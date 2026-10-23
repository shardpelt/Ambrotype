// SPDX-License-Identifier: GPL-3.0-or-later
//! Placing photos from a GPS track (a GPX file from a phone, watch or
//! logger), as Lightroom's Map module does: each photo's capture time is
//! looked up on the track, and the place there becomes its location.
//!
//! Cameras write their clock's local time with no time zone, while GPX
//! times are UTC, so the caller gives the camera clock's offset from UTC.
//! Between two track points the place is interpolated; a photo more than
//! `MAX_GAP` from any point (the logger was off) isn't placed.

use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// How far (seconds) from the nearest track point a photo may be taken
/// and still be placed.
pub const MAX_GAP: i64 = 300;

/// A track's points: UTC seconds, latitude, longitude; in time order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub points: Vec<(i64, f64, f64)>,
}

/// Read the points of every track (and route or waypoint with a time) in a
/// GPX file.
pub fn parse(text: &str) -> Result<Track, String> {
    let mut reader = Reader::from_str(text);
    let mut points = Vec::new();
    // The point being read: its place, and whether we are in its <time>.
    let mut at: Option<(f64, f64)> = None;
    let mut in_time = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.local_name();
                match name.as_ref() {
                    b"trkpt" | b"rtept" | b"wpt" => {
                        let attr = |k: &[u8]| {
                            e.attributes()
                                .flatten()
                                .find(|a| a.key.local_name().as_ref() == k)
                                .and_then(|a| a.unescape_value().ok()?.trim().parse::<f64>().ok())
                        };
                        at = attr(b"lat").zip(attr(b"lon"));
                    }
                    b"time" if at.is_some() => in_time = true,
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if in_time => {
                let text = t.unescape().map(|v| v.to_string()).unwrap_or_default();
                if let (Some((lat, lon)), Some(secs)) = (at, utc_seconds(&text)) {
                    points.push((secs, lat, lon));
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"time" => in_time = false,
                b"trkpt" | b"rtept" | b"wpt" => at = None,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("not a GPX file: {e}")),
            _ => {}
        }
    }
    if points.is_empty() {
        return Err("the GPX file has no points with times".into());
    }
    points.sort_by_key(|p| p.0);
    Ok(Track { points })
}

impl Track {
    /// The place at `utc` seconds, if the track was recording then.
    pub fn locate(&self, utc: i64) -> Option<(f64, f64)> {
        let p = &self.points;
        let i = p.partition_point(|q| q.0 <= utc);
        let before = i.checked_sub(1).map(|j| p[j]);
        let after = p.get(i).copied();
        match (before, after) {
            (Some(a), Some(b)) if b.0 - a.0 <= 2 * MAX_GAP && b.0 > a.0 => {
                let f = (utc - a.0) as f64 / (b.0 - a.0) as f64;
                Some((a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f))
            }
            (a, b) => {
                // The nearer end, if near enough.
                [a, b]
                    .into_iter()
                    .flatten()
                    .filter(|q| (q.0 - utc).abs() <= MAX_GAP)
                    .min_by_key(|q| (q.0 - utc).abs())
                    .map(|q| (q.1, q.2))
            }
        }
    }

    /// Where a photo taken at `captured` ("YYYY-MM-DD HH:MM:SS", the camera's
    /// clock) was, with the camera clock `offset_minutes` ahead of UTC.
    pub fn place(&self, captured: &str, offset_minutes: i64) -> Option<(f64, f64)> {
        self.locate(local_seconds(captured)? - offset_minutes * 60)
    }
}

/// "YYYY-MM-DD HH:MM:SS" (or with a T) as seconds since 1970, read as UTC.
pub fn local_seconds(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = s.split_once([' ', 'T']).unwrap_or((s, "00:00:00"));
    let mut d = date.split(['-', ':']).map(|v| v.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':');
    let h = t.next()?.parse::<i64>().ok()?;
    let min = t.next().unwrap_or("0").parse::<i64>().ok()?;
    let sec = t
        .next()
        .unwrap_or("0")
        .get(..2)
        .unwrap_or("0")
        .parse::<i64>()
        .unwrap_or(0);
    Some(days_from_civil(y, m, day) * 86400 + h * 3600 + min * 60 + sec)
}

/// A GPX time ("2026-07-14T10:22:33Z", maybe with fractions or an offset)
/// as UTC seconds since 1970.
fn utc_seconds(s: &str) -> Option<i64> {
    let s = s.trim();
    let (main, offset) = match s.strip_suffix('Z') {
        Some(m) => (m, 0),
        None => match s.get(19..).and_then(|rest| {
            let i = rest.find(['+', '-'])?;
            Some((19 + i, &rest[i..]))
        }) {
            Some((i, off)) => {
                let sign = if off.starts_with('-') { -1 } else { 1 };
                let (h, m) = off[1..].split_once(':').unwrap_or((&off[1..], "0"));
                (
                    &s[..i],
                    sign * (h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().ok()? * 60),
                )
            }
            None => (s, 0),
        },
    };
    let main = main.split('.').next()?;
    Some(local_seconds(main)? - offset)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    const GPX: &str = r#"<?xml version="1.0"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg>
<trkpt lat="52.0" lon="4.0"><ele>1</ele><time>2026-07-14T10:00:00Z</time></trkpt>
<trkpt lat="52.1" lon="4.2"><time>2026-07-14T10:02:00.500Z</time></trkpt>
<trkpt lat="53.0" lon="5.0"><time>2026-07-14T13:00:00+02:00</time></trkpt>
</trkseg></trk></gpx>"#;

    #[test]
    fn reads_points_in_time_order() {
        let t = parse(GPX).unwrap();
        assert_eq!(t.points.len(), 3);
        assert_eq!(local_seconds("1970-01-02 00:00:01"), Some(86401));
        // 13:00 at +02:00 is 11:00 UTC.
        assert_eq!(t.points[2].0 - t.points[0].0, 3600);
        assert!(parse("<gpx></gpx>").is_err());
    }

    #[test]
    fn places_photos_by_time_with_the_camera_offset() {
        let t = parse(GPX).unwrap();
        // Camera on UTC+2: 12:01 local is 10:01 UTC, halfway along.
        let (lat, lon) = t.place("2026-07-14 12:01:00", 120).unwrap();
        assert!(
            (lat - 52.05).abs() < 1e-3 && (lon - 4.1).abs() < 1e-3,
            "{lat} {lon}"
        );
        // Just before the track starts: its first point.
        assert_eq!(t.place("2026-07-14 09:58:00", 0), Some((52.0, 4.0)));
        // An hour-long gap between points, and long before: not placed.
        assert_eq!(t.place("2026-07-14 10:30:00", 0), None);
        assert_eq!(t.place("2026-07-13 10:00:00", 0), None);
    }
}

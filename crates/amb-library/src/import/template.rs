// SPDX-License-Identifier: GPL-3.0-or-later
//! Import's names: the capture time, and the folder and file name templates.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct When {
    pub(super) y: i64,
    pub(super) mo: u32,
    pub(super) d: u32,
    pub(super) h: u32,
    pub(super) mi: u32,
    pub(super) s: u32,
}

impl When {
    pub(super) fn text(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.y, self.mo, self.d, self.h, self.mi, self.s
        )
    }

    /// EXIF's "YYYY:MM:DD HH:MM:SS".
    pub(super) fn from_exif(t: &str) -> Option<When> {
        let n: Vec<i64> = t
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        let [y, mo, d, h, mi, s] = n.get(..6)?.try_into().ok()?;
        (y > 1900 && (1..=12).contains(&mo) && (1..=31).contains(&d)).then_some(When {
            y,
            mo: mo as u32,
            d: d as u32,
            h: h as u32,
            mi: mi as u32,
            s: s as u32,
        })
    }

    /// Seconds since 1970 (UTC) as a civil date.
    pub(super) fn from_unix(secs: i64) -> When {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Howard Hinnant's days-to-civil.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = yoe + era * 400 + i64::from(mo <= 2);
        When {
            y,
            mo,
            d,
            h: (rem / 3600) as u32,
            mi: (rem % 3600 / 60) as u32,
            s: (rem % 60) as u32,
        }
    }
}

/// Check a template: every `{...}` must be a known token.
pub fn check_template(template: &str) -> Result<(), String> {
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        let end = rest[i..]
            .find('}')
            .ok_or_else(|| format!("{template:?}: a {{ without its }}"))?;
        let token = &rest[i..i + end + 1];
        if !TOKENS.iter().any(|(t, _)| *t == token) {
            let known: Vec<&str> = TOKENS.iter().map(|(t, _)| *t).collect();
            return Err(format!(
                "{template:?}: unknown {token}; use {}",
                known.join(" ")
            ));
        }
        rest = &rest[i + end + 1..];
    }
    Ok(())
}

/// A file or folder name part: no separators or characters Windows refuses.
pub(crate) fn clean(part: &str) -> String {
    let s: String = part
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    s.trim().trim_matches('.').to_string()
}

pub(super) fn expand(template: &str, name: &str, when: &When, seq: u32, camera: &str) -> String {
    template
        .replace("{name}", name)
        .replace("{yyyy}", &format!("{:04}", when.y))
        .replace("{yy}", &format!("{:02}", when.y.rem_euclid(100)))
        .replace("{mm}", &format!("{:02}", when.mo))
        .replace("{dd}", &format!("{:02}", when.d))
        .replace("{hh}", &format!("{:02}", when.h))
        .replace("{min}", &format!("{:02}", when.mi))
        .replace("{ss}", &format!("{:02}", when.s))
        .replace("{seq}", &format!("{seq:04}"))
        .replace("{camera}", camera)
}

/// A file name (no extension) for `photo` from a name `template`, with
/// `seq` for `{seq}`: what Import uses, and exports with a name template.
/// `{name}` is the photo's own name; the capture time comes from its EXIF.
pub fn file_name(template: &str, photo: &Path, seq: u32) -> Result<String, String> {
    check_template(template)?;
    let (when, camera) = capture_time(&amb_engine::source_path(photo));
    let name = photo
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let out = clean(&expand(template, &name, &when, seq, &camera));
    if out.is_empty() {
        return Err(format!("{template:?} gives an empty name"));
    }
    Ok(out)
}

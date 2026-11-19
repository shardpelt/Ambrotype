use super::read::read_info;
use super::write::{collect_foreign, merge, with_info};
use super::*;
use amb_presets::Value;
use std::collections::BTreeMap;

#[test]
fn the_orientation_is_written_where_lightroom_keeps_it() {
    let turned = PhotoInfo {
        orientation: Some(6),
        ..read_info(SIDECAR)
    };
    let out = with_info(Some(SIDECAR), &turned).unwrap();
    assert!(out.contains(r#"tiff:Orientation="6""#), "{out}");
    assert_eq!(read_info(&out).orientation, Some(6));
    // Turned again: one value, the new one; other info stays.
    let again = with_info(
        Some(&out),
        &PhotoInfo {
            orientation: Some(3),
            ..read_info(&out)
        },
    )
    .unwrap();
    assert_eq!(again.matches("Orientation").count(), 1, "{again}");
    assert_eq!(read_info(&again).orientation, Some(3));
    assert_eq!(read_info(&again).rating, read_info(SIDECAR).rating);
    // Not saying keeps it.
    let kept = with_info(
        Some(&again),
        &PhotoInfo {
            orientation: None,
            ..read_info(&again)
        },
    )
    .unwrap();
    assert_eq!(read_info(&kept).orientation, Some(3));
}

#[test]
fn places_round_trip_in_lightrooms_form() {
    let xmp = with_info(
        None,
        &PhotoInfo {
            gps: Some("52.3676,-4.9041".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(xmp.contains("GPSLatitude=\"52,22.056000N\""), "{xmp}");
    assert!(xmp.contains("GPSLongitude=\"4,54.246000W\""), "{xmp}");
    let (lat, lon) = read_info(&xmp).place().unwrap();
    assert!((lat - 52.3676).abs() < 1e-6 && (lon + 4.9041).abs() < 1e-6);
    // Kept by other changes, removed by "".
    let rated = with_info(
        Some(&xmp),
        &PhotoInfo {
            rating: Some(3),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(read_info(&rated).place().is_some());
    let gone = with_info(
        Some(&rated),
        &PhotoInfo {
            gps: Some(String::new()),
            ..read_info(&rated)
        },
    )
    .unwrap();
    assert_eq!(read_info(&gone).gps, None, "{gone}");
    // Lightroom's element form, with seconds.
    let lr = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:exif="http://ns.adobe.com/exif/1.0/">
<exif:GPSLatitude>33,51,30S</exif:GPSLatitude><exif:GPSLongitude>151,12.5E</exif:GPSLongitude>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let (lat, lon) = read_info(lr).place().unwrap();
    assert!(
        (lat + 33.858333).abs() < 1e-5 && (lon - 151.208333).abs() < 1e-5,
        "{lat} {lon}"
    );
}

#[test]
fn stacks_round_trip_and_survive_other_changes() {
    let top = StackRef {
        id: "s1".into(),
        top: true,
    };
    let xmp = with_info(
        None,
        &PhotoInfo {
            stack: Some(top.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(read_info(&xmp).stack, Some(top.clone()));
    let rated = with_info(
        Some(&xmp),
        &PhotoInfo {
            rating: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(read_info(&rated).stack, Some(top));
    let moved = StackRef {
        id: "s1".into(),
        top: false,
    };
    let xmp = with_info(
        Some(&rated),
        &PhotoInfo {
            stack: Some(moved.clone()),
            ..read_info(&rated)
        },
    )
    .unwrap();
    assert_eq!(read_info(&xmp).stack, Some(moved));
    assert!(!xmp.contains("StackTop"), "{xmp}");
    let out = with_info(
        Some(&xmp),
        &PhotoInfo {
            stack: Some(StackRef::default()),
            ..read_info(&xmp)
        },
    )
    .unwrap();
    assert_eq!(read_info(&out).stack, None);
    assert_eq!(read_info(&out).rating, Some(2));
}

#[test]
fn title_caption_creator_and_copyright_round_trip() {
    let info = PhotoInfo {
        rating: Some(3),
        title: Some("Harbour at dawn".into()),
        caption: Some("Boats & gulls <early>".into()),
        creator: Some("Ann; Bo".into()),
        copyright: Some("© 2026 Ann".into()),
        ..Default::default()
    };
    let xmp = with_info(None, &info).unwrap();
    assert!(
        xmp.contains("<rdf:Seq><rdf:li>Ann</rdf:li><rdf:li>Bo</rdf:li></rdf:Seq>"),
        "{xmp}"
    );
    let back = read_info(&xmp);
    assert_eq!(back.title.as_deref(), Some("Harbour at dawn"));
    assert_eq!(back.caption.as_deref(), Some("Boats & gulls <early>"));
    assert_eq!(back.creator.as_deref(), Some("Ann; Bo"));
    assert_eq!(back.copyright.as_deref(), Some("© 2026 Ann"));
    // Changing only the rating keeps them; "" removes one.
    let rated = with_info(
        Some(&xmp),
        &PhotoInfo {
            rating: Some(5),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(read_info(&rated).title.as_deref(), Some("Harbour at dawn"));
    let cleared = with_info(
        Some(&rated),
        &PhotoInfo {
            title: Some(String::new()),
            ..read_info(&rated)
        },
    )
    .unwrap();
    let c = read_info(&cleared);
    assert_eq!(c.title, None);
    assert_eq!(c.copyright.as_deref(), Some("© 2026 Ann"));
    assert_eq!(c.rating, Some(5));
}

const SIDECAR: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    xmp:Rating="4" xmp:Label="Red &amp; Blue"
    crs:Exposure2012="+0.65" crs:Temperature="5400">
   <dc:subject><rdf:Bag><rdf:li>sunset</rdf:li><rdf:li>beach</rdf:li></rdf:Bag></dc:subject>
   <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

#[test]
fn rating_and_keywords_round_trip() {
    let info = read_info(SIDECAR);
    assert_eq!(info.rating, Some(4));
    assert_eq!(info.keywords, ["sunset", "beach"]);
    assert_eq!(info.label.as_deref(), Some("Red & Blue"));

    // New rating and keywords, same label; everything else (develop settings) stays.
    let new = PhotoInfo {
        rating: Some(2),
        keywords: vec!["sunset".into(), " family ".into(), "Sunset".into()],
        label: info.label.clone(),
        flag: None,
        ..Default::default()
    };
    let out = with_info(Some(SIDECAR), &new).unwrap();
    let back = read_info(&out);
    assert_eq!(back.rating, Some(2));
    assert_eq!(back.keywords, ["sunset", "family"]);
    assert!(out.contains(r#"xmp:Label="Red &amp; Blue""#), "{out}");
    assert!(!out.contains("beach"));
    let settings = amb_presets::xmp::parse_crs(&out).unwrap();
    assert!(settings.contains_key("Exposure2012") && settings.contains_key("ToneCurvePV2012"));

    // Clearing removes them; a sidecar that didn't exist gets created.
    let cleared = with_info(Some(&out), &PhotoInfo::default()).unwrap();
    assert_eq!(read_info(&cleared), PhotoInfo::default());
    let both = PhotoInfo {
        rating: Some(5),
        keywords: vec!["a & b".into()],
        label: Some("Green".into()),
        flag: None,
        ..Default::default()
    };
    let fresh = with_info(None, &both).unwrap();
    assert_eq!(read_info(&fresh), both);
    assert!(fresh.contains(r#"xmp:Label="Green""#), "{fresh}");
    // A new label replaces the old one; an empty one removes it.
    let blue = with_info(
        Some(&fresh),
        &PhotoInfo {
            label: Some("Blue".into()),
            ..both.clone()
        },
    )
    .unwrap();
    assert_eq!(read_info(&blue).label.as_deref(), Some("Blue"));
    assert_eq!(blue.matches("Label").count(), 1, "{blue}");
    let none = with_info(
        Some(&blue),
        &PhotoInfo {
            label: Some("  ".into()),
            ..both
        },
    )
    .unwrap();
    assert_eq!(read_info(&none).label, None);
}

#[test]
fn flags_round_trip_beside_the_stars() {
    // Picked, with stars.
    let picked = PhotoInfo {
        rating: Some(3),
        flag: Some(Flag::Pick),
        ..PhotoInfo::default()
    };
    let out = with_info(Some(SIDECAR), &picked).unwrap();
    assert!(out.contains(r#"amblib:Flag="pick""#), "{out}");
    assert_eq!(read_info(&out).flag, Some(Flag::Pick));
    assert_eq!(read_info(&out).rating, Some(3));
    // Rejected without stars: Bridge's -1 as well.
    let rejected = PhotoInfo {
        flag: Some(Flag::Reject),
        ..PhotoInfo::default()
    };
    let out = with_info(Some(&out), &rejected).unwrap();
    assert!(out.contains(r#"xmp:Rating="-1""#), "{out}");
    assert_eq!(out.matches("Flag=").count(), 1, "{out}");
    assert_eq!(read_info(&out), rejected);
    // Unflagged: both go.
    let out = with_info(Some(&out), &PhotoInfo::default()).unwrap();
    assert!(!out.contains("Flag") && !out.contains("Rating"), "{out}");
    // Another app's -1 reads as rejected.
    let bridge = SIDECAR.replace(r#"xmp:Rating="4""#, r#"xmp:Rating="-1""#);
    let info = read_info(&bridge);
    assert_eq!((info.rating, info.flag), (None, Some(Flag::Reject)));
}

#[test]
fn keyword_paths_follow_the_keywords() {
    let lr = SIDECAR.replace(
            "<dc:subject>",
            r#"<lr:hierarchicalSubject xmlns:lr="http://ns.adobe.com/lightroom/1.0/"><rdf:Bag><rdf:li>Places|beach</rdf:li><rdf:li>Time|sunset</rdf:li></rdf:Bag></lr:hierarchicalSubject>
   <dc:subject>"#,
        );
    let info = PhotoInfo {
        rating: Some(4),
        keywords: vec!["sunset".into()],
        label: None,
        flag: None,
        ..Default::default()
    };
    let out = with_info(Some(&lr), &info).unwrap();
    assert!(
        out.contains("Time|sunset") && !out.contains("Places|beach"),
        "{out}"
    );
}

#[test]
fn keeps_foreign_metadata() {
    let mut settings = amb_presets::xmp::parse_crs(SIDECAR).unwrap();
    settings.insert("Exposure2012".into(), Value::Number(1.25));
    settings.remove("ToneCurvePV2012");
    let out = merge(SIDECAR, &settings).unwrap();
    assert!(out.contains(r#"xmp:Rating="4""#), "{out}");
    assert!(out.contains(r#"xmp:Label="Red &amp; Blue""#), "{out}");
    assert!(out.contains("<rdf:li>sunset</rdf:li>"), "{out}");
    assert!(out.contains(r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#));
    assert!(!out.contains("ToneCurvePV2012"), "{out}");
    let back = amb_presets::xmp::parse_crs(&out).unwrap();
    assert_eq!(back["Exposure2012"].as_f64(), Some(1.25));
    assert_eq!(back["Temperature"].as_f64(), Some(5400.0));
    // Still well-formed and the foreign values survive a parse.
    let again = collect_foreign(&out).unwrap();
    assert_eq!(again.attributes.len(), 2);
    assert_eq!(again.elements.len(), 1);
}

#[test]
fn self_closing_description_gets_children() {
    let mut settings = BTreeMap::new();
    settings.insert("Vibrance".to_string(), Value::Number(10.0));
    let out = merge(SIDECAR, &settings).unwrap();
    assert!(out.contains("</rdf:Description>"));
    assert!(collect_foreign(&out).unwrap().elements.len() == 1);
    assert_eq!(
        amb_presets::xmp::parse_crs(&out).unwrap()["Vibrance"].as_f64(),
        Some(10.0)
    );
}

#[test]
fn whitespace_in_foreign_attributes_survives() {
    let src = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" photoshop:Instructions="line1&#xA;line2&#xD;&#x9;end" crs:Exposure2012="+0.30"/></rdf:RDF></x:xmpmeta>"#;
    let mut settings = BTreeMap::new();
    settings.insert("Exposure2012".to_string(), Value::Number(0.5));
    let out = merge(src, &settings).unwrap();
    assert!(!out.contains("line1\nline2"), "{out}");
    assert!(out.contains("line1&#xA;line2&#xD;&#x9;end"), "{out}");
    let again = collect_foreign(&out).unwrap();
    assert_eq!(again.attributes[0].1, "line1\nline2\r\tend");
}

#[test]
fn plain_crs_file_is_rewritten_cleanly() {
    let src = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Sharpness="+80"/></rdf:RDF></x:xmpmeta>"#;
    let mut settings = BTreeMap::new();
    settings.insert("Sharpness".to_string(), Value::Number(50.0));
    let out = merge(src, &settings).unwrap();
    assert_eq!(
        out,
        amb_presets::xmp_writer::write_packet(&settings, amb_presets::is_signed_tag)
    );
}

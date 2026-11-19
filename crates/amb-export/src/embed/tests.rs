use super::avif::{Meta, av1_set_colour, avif_set_colour, item_extents};
use super::bmff::{boxes, read_be};
use super::jxl::jxl_with_boxes;
use super::*;
use std::collections::BTreeMap;

fn tags() -> Tags {
    Tags {
        rating: Some(4),
        keywords: vec!["zee".into(), "Strand & duin".into()],
        label: Some("Green".into()),
        title: Some("Duinen".into()),
        creator: Some("Ann; Bo".into()),
        ..Default::default()
    }
}

#[test]
fn packet_and_iptc() {
    let p = xmp_packet(&tags()).unwrap();
    assert!(p.contains(r#"xmp:Rating="4""#) && p.contains("<rdf:li>Strand &amp; duin</rdf:li>"));
    assert!(p.contains(r#"xmp:Label="Green""#), "{p}");
    assert!(
        p.contains(r#"<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Duinen</rdf:li>"#),
        "{p}"
    );
    assert!(
        p.contains("<dc:creator><rdf:Seq><rdf:li>Ann</rdf:li><rdf:li>Bo</rdf:li></rdf:Seq>"),
        "{p}"
    );
    assert!(xmp_packet(&Tags::default()).is_none());
    let iptc = iptc_app13(&["x".repeat(100)]).unwrap();
    assert!(iptc.starts_with(b"Photoshop 3.0\0"));
    assert!(
        iptc.windows(2).any(|w| w == [0x00, 64]),
        "keywords are cut to 64 bytes"
    );
}

fn avif(exif: bool) -> Vec<u8> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    let mut enc = image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut buf, 10, 80);
    if exif {
        let block = crate::exif::build_with_tags(
            &Default::default(),
            crate::exif::MetadataPolicy::All,
            &tags(),
        )
        .unwrap();
        enc.set_exif_metadata(block).unwrap();
    }
    let rgb: Vec<u8> = (0..4 * 4 * 3).map(|i| (i * 13) as u8).collect();
    enc.write_image(&rgb, 4, 4, image::ExtendedColorType::Rgb8)
        .unwrap();
    buf
}

/// Each item's data, read back through `iloc`, and the `meta` children.
fn items(bytes: &[u8]) -> (BTreeMap<u64, Vec<u8>>, Vec<[u8; 4]>) {
    let top = boxes(bytes, 0, bytes.len()).unwrap();
    let meta = top.iter().find(|b| &b.kind == b"meta").unwrap();
    let children = boxes(bytes, meta.body + 4, meta.end).unwrap();
    let iloc = children.iter().find(|c| &c.kind == b"iloc").unwrap();
    let b = &bytes[iloc.body..iloc.end];
    assert_eq!((b[0], b[4], b[5]), (0, 0x44, 0), "iloc v0, 4-byte offsets");
    let mut at = 6;
    let mut out = BTreeMap::new();
    for _ in 0..read_be(b, &mut at, 2).unwrap() {
        let id = read_be(b, &mut at, 2).unwrap();
        at += 2;
        let mut data = Vec::new();
        for _ in 0..read_be(b, &mut at, 2).unwrap() {
            let off = read_be(b, &mut at, 4).unwrap() as usize;
            let len = read_be(b, &mut at, 4).unwrap() as usize;
            data.extend_from_slice(&bytes[off..off + len]);
        }
        out.insert(id, data);
    }
    (out, children.iter().map(|c| c.kind).collect())
}

#[test]
fn avif_gets_an_xmp_item() {
    let xmp = xmp_packet(&tags()).unwrap();
    for exif in [false, true] {
        let before = avif(exif);
        let after = embed(Format::Avif, before.clone(), &tags());
        let (old, _) = items(&before);
        let (new, kinds) = items(&after);
        // The image (and the EXIF) still read the same, and the XMP is there.
        for (id, data) in &old {
            assert_eq!(&new[id], data, "item {id}");
        }
        let added = new.keys().max().unwrap();
        assert_eq!(new[added], xmp.as_bytes());
        assert!(kinds.contains(b"iref"), "{kinds:?}");
        let text = String::from_utf8_lossy(&after);
        assert!(text.contains("mime\0application/rdf+xml\0"));
        assert!(text.contains("cdsc"));
    }
    // Damaged files are left as they are, never a panic.
    let good = avif(true);
    let mut seed = 0x2545_f491u64;
    for _ in 0..2000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut bad = good.clone();
        let i = (seed % bad.len() as u64) as usize;
        bad[i] = (seed >> 32) as u8;
        if seed % 5 == 0 {
            bad.truncate(i);
        }
        let _ = embed(Format::Avif, bad, &tags());
    }
}

#[test]
fn avif_colour_is_named_in_the_box_and_the_av1_data() {
    let named = avif_set_colour(avif(true), (12, 13)).unwrap();
    let m = Meta::read(&named).unwrap();
    // Every sequence header (in the image, and in `av1C`) says P3 / sRGB.
    let mut ranges = item_extents(&m, m.primary).unwrap();
    let p = named.windows(4).position(|w| w == b"av1C").unwrap();
    let size = u32::from_be_bytes(named[p - 4..p].try_into().unwrap()) as usize;
    // `av1C` may carry no copy of the header.
    if size > 12 {
        ranges.push((p + 8, size - 12));
    }
    for (start, len) in ranges {
        let mut obus = named[start..start + len].to_vec();
        // Writing what is there changes nothing.
        assert_eq!(av1_set_colour(&mut obus, 12, 13), 1);
        assert_eq!(obus, &named[start..start + len]);
    }
    let image = avif(true);
    let (old, _) = items(&image);
    let (new, _) = items(&named);
    assert_eq!(old.len(), new.len());
    // Damaged files: refused or named, never a panic.
    let mut seed = 0x9e37_79b9u64;
    for _ in 0..2000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut bad = image.clone();
        let i = (seed % bad.len() as u64) as usize;
        bad[i] = (seed >> 32) as u8;
        let _ = avif_set_colour(bad, (9, 1));
    }
}

#[test]
fn jxl_boxes_go_before_the_image() {
    let code = [0xFF, 0x0A, 1, 2, 3];
    let one = jxl_with_boxes(&code, &[(*b"Exif", b"\0\0\0\0II*\0")]).unwrap();
    let two = embed(Format::Jxl, one, &tags());
    let kinds: Vec<_> = boxes(&two, 0, two.len())
        .unwrap()
        .iter()
        .map(|b| b.kind)
        .collect();
    assert_eq!(kinds, [*b"JXL ", *b"ftyp", *b"Exif", *b"xml ", *b"jxlc"]);
    assert!(two.ends_with(&code));
    assert!(jxl_with_boxes(b"not jxl", &[]).is_none());
}

#[test]
fn crc_matches_the_png_spec() {
    assert_eq!(crc32(b"IEND"), 0xAE42_6082);
}

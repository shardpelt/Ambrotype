// SPDX-License-Identifier: GPL-3.0-or-later
//! Import's tests.

use super::*;
use amb_protocol::sidecar;

#[test]
fn export_names_come_from_the_template() {
    let dir = std::env::temp_dir().join(format!("amb-import-names-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let photo = dir.join("IMG_0001.dng");
    std::fs::write(&photo, b"not a raw").unwrap();
    assert_eq!(file_name("{name}", &photo, 1).unwrap(), "IMG_0001");
    assert_eq!(file_name("trip_{seq}", &photo, 7).unwrap(), "trip_0007");
    // The file's time when there's no EXIF; a name with a slash is cleaned.
    let n = file_name("{yyyy}/{name}", &photo, 1).unwrap();
    assert!(n.ends_with("-IMG_0001") && !n.contains('/'), "{n}");
    assert!(file_name("{nope}", &photo, 1).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("amb-import-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn dates_and_templates() {
    assert_eq!(When::from_unix(1_760_097_600).text(), "2025-10-10 12:00:00");
    assert_eq!(When::from_unix(0).text(), "1970-01-01 00:00:00");
    let w = When::from_exif("2026:05:04 07:08:09").unwrap();
    assert_eq!(
        expand(
            "{yyyy}/{yy}{mm}{dd}_{hh}{min}{ss}_{seq}_{name}",
            "IMG_1",
            &w,
            7,
            "X100V"
        ),
        "2026/260504_070809_0007_IMG_1"
    );
    assert!(When::from_exif("0000:00:00 00:00:00").is_none());
    assert!(
        check_template("{yyyy}-{month}")
            .unwrap_err()
            .contains("{month}")
    );
    assert!(check_template("{yyyy").is_err());
    assert_eq!(clean(" a:b/c. "), "a-b-c");
}

#[test]
fn plans_groups_renames_and_skips_duplicates() {
    let root = temp("plan");
    let card = root.join("card/DCIM/100CANON");
    let lib = root.join("photos");
    write(&card.join("IMG_0002.CR2"), b"raw two");
    write(&card.join("IMG_0002.JPG"), b"jpeg two");
    write(
        &card.join("IMG_0002.xmp"),
        br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="3"/></rdf:RDF></x:xmpmeta>"#,
    );
    write(&card.join("IMG_0002.copy-2.xmp"), b"a virtual copy");
    write(&card.join("IMG_0001.CR2"), b"raw one");
    write(&card.join("phone.jpg"), b"phone photo");
    write(
        &card.join("phone.jpg.xmp"),
        br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="2"/></rdf:RDF></x:xmpmeta>"#,
    );
    write(&card.join("notes.txt"), b"not a photo");
    write(&card.join(".hidden.CR2"), b"hidden");
    // Same name already in the library, different photo: renamed.
    // Same name and bytes: skipped.
    let options = ImportOptions {
        folder: "shoot".into(),
        ..ImportOptions::default()
    };
    write(&lib.join("shoot/IMG_0001.CR2"), b"raw one");
    write(&lib.join("shoot/IMG_0002.CR2"), b"another photo");
    let plan = plan(&root.join("card"), &lib, &options).unwrap();
    assert_eq!(plan.len(), 3, "{plan:#?}");
    let phone = plan
        .iter()
        .find(|p| p.files[0].0.ends_with("phone.jpg"))
        .unwrap();
    assert!(
        phone.files[1].1.ends_with("shoot/phone.jpg.xmp"),
        "{phone:#?}"
    );
    let one = plan
        .iter()
        .find(|p| p.files[0].0.ends_with("IMG_0001.CR2"))
        .unwrap();
    assert!(one.skip.as_deref().unwrap().contains("already imported"));
    let two = plan
        .iter()
        .find(|p| p.files[0].0.ends_with("IMG_0002.CR2"))
        .unwrap();
    assert!(two.skip.is_none());
    let dests: Vec<String> = two
        .files
        .iter()
        .map(|(_, d)| d.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(dests.len(), 4);
    assert!(
        dests.contains(&"IMG_0002-2.copy-2.xmp".to_string()),
        "{dests:?}"
    );
    assert!(
        dests.iter().all(|d| d.starts_with("IMG_0002-2.")),
        "{dests:?}"
    );

    let report = run(
        &plan,
        &ImportOptions {
            keywords: vec!["trip".into()],
            ..options.clone()
        },
        &mut |_, _, _| {},
    );
    assert_eq!((report.imported, report.skipped), (2, 1), "{report:?}");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let shoot = lib.join("shoot");
    assert_eq!(
        std::fs::read(shoot.join("IMG_0002-2.JPG")).unwrap(),
        b"jpeg two"
    );
    let xmp = std::fs::read_to_string(shoot.join("IMG_0002-2.xmp")).unwrap();
    let phone = sidecar::read_info(&std::fs::read_to_string(shoot.join("phone.jpg.xmp")).unwrap());
    assert_eq!(
        (phone.keywords, phone.rating),
        (vec!["trip".to_string()], Some(2))
    );
    assert!(!shoot.join("phone.xmp").exists());
    let info = sidecar::read_info(&xmp);
    assert_eq!(
        (info.keywords, info.rating),
        (vec!["trip".to_string()], Some(3))
    );
    assert_eq!(report.folders, std::slice::from_ref(&shoot));
    // No temporary files left; a second run imports nothing new.
    assert!(
        std::fs::read_dir(&shoot)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().starts_with('.'))
    );
    let again = super::plan(&root.join("card"), &lib, &options).unwrap();
    assert!(again.iter().all(|p| p.skip.is_some()), "{again:#?}");
    // Numbered names: a new photo first in line moves every number, but
    // the photos already imported are still found.
    let numbered = ImportOptions {
        name: "s_{seq}".into(),
        ..options.clone()
    };
    let first = super::plan(&root.join("card"), &lib, &numbered).unwrap();
    run(&first, &numbered, &mut |_, _, _| {});
    write(&card.join("IMG_0000.CR2"), b"raw zero");
    let mtime = std::fs::metadata(card.join("IMG_0001.CR2"))
        .unwrap()
        .modified()
        .unwrap();
    std::fs::File::options()
        .write(true)
        .open(card.join("IMG_0000.CR2"))
        .unwrap()
        .set_modified(mtime - std::time::Duration::from_secs(3600))
        .unwrap();
    let next = super::plan(&root.join("card"), &lib, &numbered).unwrap();
    let fresh: Vec<_> = next.iter().filter(|p| p.skip.is_none()).collect();
    assert_eq!(fresh.len(), 1, "{next:#?}");
    assert!(fresh[0].files[0].0.ends_with("IMG_0000.CR2"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn presets_and_backup_copies() {
    let root = temp("preset");
    let card = root.join("card");
    let lib = root.join("photos");
    write(&card.join("A.CR2"), b"raw a");
    write(&card.join("B.CR2"), b"raw b");
    // B was edited in the camera's app already: its edit stays.
    write(
        &card.join("B.xmp"),
        br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="-1.00"/></rdf:RDF></x:xmpmeta>"#,
    );
    let preset = root.join("bright.xmp");
    write(
        &preset,
        br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="+0.80" crs:Contrast2012="+15"/></rdf:RDF></x:xmpmeta>"#,
    );
    let options = ImportOptions {
        folder: String::new(),
        keywords: vec!["trip".into()],
        preset: Some(preset),
        backup: Some(root.join("backup")),
        ..ImportOptions::default()
    };
    let plan = plan(&card, &lib, &options).unwrap();
    let report = run(&plan, &options, &mut |_, _, _| {});
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.imported, 2);
    let read = |name: &str| std::fs::read_to_string(lib.join(name)).unwrap();
    let a = amb_presets::xmp::parse_crs(&read("A.xmp")).unwrap();
    assert_eq!(a["Exposure2012"].as_f64(), Some(0.8));
    assert_eq!(a["Contrast2012"].as_f64(), Some(15.0));
    assert_eq!(sidecar::read_info(&read("A.xmp")).keywords, ["trip"]);
    let b = amb_presets::xmp::parse_crs(&read("B.xmp")).unwrap();
    assert_eq!(b["Exposure2012"].as_f64(), Some(-1.0));
    assert!(!b.contains_key("Contrast2012"));
    // The originals, as they were, in today's backup folder.
    let day = std::fs::read_dir(root.join("backup"))
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    assert!(
        day.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("Imported on ")
    );
    assert_eq!(std::fs::read(day.join("A.CR2")).unwrap(), b"raw a");
    assert!(day.join("B.xmp").exists());
    // Again: nothing is doubled in the backup.
    run(&plan, &options, &mut |_, _, _| {});
    assert_eq!(std::fs::read_dir(&day).unwrap().count(), 3);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sequence_names_follow_capture_order() {
    let root = temp("seq");
    let card = root.join("card");
    write(&card.join("b.nef"), b"b");
    write(&card.join("a.nef"), b"a");
    // Without EXIF the modified time orders them: b first.
    let early = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let late = early + std::time::Duration::from_secs(60);
    for (name, t) in [("b.nef", early), ("a.nef", late)] {
        std::fs::File::options()
            .write(true)
            .open(card.join(name))
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
    let options = ImportOptions {
        folder: "{yyyy}-{mm}".into(),
        name: "trip_{seq}".into(),
        first_number: 9,
        ..ImportOptions::default()
    };
    let plan = plan(&card, &root.join("lib"), &options).unwrap();
    let names: Vec<String> = plan
        .iter()
        .map(|p| {
            let (src, dst) = &p.files[0];
            format!(
                "{} {}",
                src.file_name().unwrap().to_string_lossy(),
                // `/` between folders on every system, for the comparison.
                dst.strip_prefix(root.join("lib"))
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            )
        })
        .collect();
    assert_eq!(
        names,
        ["b.nef 2023-11/trip_0009.nef", "a.nef 2023-11/trip_0010.nef"]
    );
    let _ = std::fs::remove_dir_all(&root);
}

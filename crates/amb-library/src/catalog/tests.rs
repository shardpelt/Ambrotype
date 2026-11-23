use super::*;
use amb_protocol::sidecar::{self, PhotoInfo};
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("amb-catalog-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(photo: &Path, info: &PhotoInfo) {
    let text = sidecar::with_info(None, info).unwrap();
    sidecar::write_atomic(&amb_engine::sidecar_path(photo), &text).unwrap();
}

fn info(rating: u8, keyword: &str) -> PhotoInfo {
    PhotoInfo {
        rating: Some(rating),
        keywords: vec![keyword.into()],
        ..Default::default()
    }
}

#[test]
fn text_search_finds_a_photo_by_its_place_names() {
    let entry = Entry {
        path: PathBuf::from("x/IMG_1.jpg"),
        record: Record {
            info: PhotoInfo {
                city: Some("Lisbon".into()),
                country: Some("Portugal".into()),
                ..Default::default()
            },
            ..Record::default()
        },
        offline: false,
    };
    let q = |t: &str| Query {
        text: Some(t.into()),
        ..Query::default()
    };
    assert!(q("lisb").matches(&entry) && q("PORTUGAL").matches(&entry));
    assert!(!q("madrid").matches(&entry));
}

#[test]
fn records_follow_the_files() {
    let dir = temp_dir("follow");
    let photo = dir.join("a.jpg");
    std::fs::write(&photo, b"not really a jpeg").unwrap();
    let cat = Catalog::in_memory();
    assert_eq!(cat.record(&photo).info, PhotoInfo::default());
    assert_eq!(
        cat.stats(),
        Stats {
            photos: 1,
            folders: 1
        }
    );

    // Another app writes the sidecar: the change is seen.
    write(&photo, &info(4, "sea"));
    let r = cat.record(&photo);
    assert_eq!(r.info.rating, Some(4));
    assert_eq!(r.info.keywords, ["sea"]);

    // Unchanged files come from the catalogue, not the disk: a record
    // planted in it is what comes back.
    let mut planted = r.clone();
    planted.info.rating = Some(1);
    cat.store(std::iter::once((&photo, stamp(&photo).unwrap(), &planted)));
    assert_eq!(cat.record(&photo).info.rating, Some(1));

    // Gone from its folder: forgotten when the folder is indexed.
    std::fs::remove_file(&photo).unwrap();
    let r = cat.index(&dir, &|_, _| {}).unwrap();
    assert_eq!((r.photos, r.forgotten), (0, 1));
    assert!(cat.all().is_empty());
    assert_eq!(cat.stats().photos, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_catalogue_survives_reopening_and_folders_can_be_forgotten() {
    let dir = temp_dir("reopen");
    let sub = dir.join("2026-07");
    std::fs::create_dir_all(&sub).unwrap();
    let photos = [dir.join("a.jpg"), sub.join("b.jpg")];
    for p in &photos {
        std::fs::write(p, b"x").unwrap();
        write(p, &info(3, "hill"));
    }
    let file = dir.join("catalog.sqlite");
    {
        let cat = Catalog::open(&file).unwrap();
        assert_eq!(cat.records(&photos)[1].info.rating, Some(3));
    }
    let cat = Catalog::open(&file).unwrap();
    assert_eq!(
        cat.stats(),
        Stats {
            photos: 2,
            folders: 2
        }
    );
    assert_eq!(cat.all().len(), 2);
    assert_eq!(cat.forget_folder(&sub), 1);
    assert_eq!(cat.forget_folder(&dir), 1);
    assert_eq!(cat.stats().photos, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn photos_on_an_unplugged_drive_stay_listed_as_offline() {
    let dir = temp_dir("offline");
    let drive = dir.join("drive");
    let shoot = drive.join("2026").join("07");
    std::fs::create_dir_all(&shoot).unwrap();
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        std::fs::write(shoot.join(name), b"x").unwrap();
    }
    write(&shoot.join("a.jpg"), &info(5, "lake"));
    std::fs::write(drive.join("d.jpg"), b"x").unwrap();
    let cat = Catalog::in_memory();
    let reports = std::sync::Mutex::new(Vec::new());
    let r = cat
        .index(&drive, &|done, total| {
            reports.lock().unwrap().push((done, total))
        })
        .unwrap();
    assert_eq!((r.photos, r.read, r.forgotten, r.offline), (4, 4, 0, false));
    assert_eq!(reports.lock().unwrap().last(), Some(&(4, 4)));
    let counts: Vec<(PathBuf, usize)> = cat
        .folders()
        .into_iter()
        .map(|f| (f.folder, f.photos))
        .collect();
    assert_eq!(counts, [(drive.clone(), 1), (shoot.clone(), 3)]);

    // The drive is unplugged: nothing is forgotten, all of it is offline.
    let away = dir.join("away");
    std::fs::rename(&drive, &away).unwrap();
    assert!(cat.index(&drive, &|_, _| {}).unwrap().offline);
    let all = cat.all();
    assert_eq!(all.len(), 4);
    assert!(all.iter().all(|e| e.offline));
    let a = all.iter().find(|e| e.path.ends_with("a.jpg")).unwrap();
    assert_eq!(a.record.info.rating, Some(5));
    let s = cat.stamp_of(&a.path).unwrap();
    assert_eq!(s.size, 1);
    assert!(s.sidecar_size > 0);
    assert!(cat.folders().iter().all(|f| f.offline));

    // Plugged back in, with one photo deleted meanwhile.
    std::fs::rename(&away, &drive).unwrap();
    std::fs::remove_file(shoot.join("c.jpg")).unwrap();
    let r = cat.index(&drive, &|_, _| {}).unwrap();
    assert_eq!((r.photos, r.read, r.forgotten), (3, 0, 1));
    assert!(cat.all().iter().all(|e| !e.offline));
    let _ = std::fs::remove_dir_all(&dir);
}

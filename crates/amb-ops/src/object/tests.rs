use super::geometry::{bounds, cover, shadow_of};
use super::*;

#[test]
fn dabs_cover_a_shape_and_leave_its_gaps() {
    // Two blobs with a wide gap between them.
    let (w, h) = (120, 40);
    let mut mask = vec![false; w * h];
    for y in 10..30 {
        for x in (10..30).chain(90..110) {
            mask[y * w + x] = true;
        }
    }
    let r = 6.0;
    let dabs = cover(&mask, w, h, r);
    // Every masked pixel is under a dab.
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                let (px, py) = (x as f64, y as f64);
                assert!(
                    dabs.iter().any(|&(dx, dy)| (dx - px).hypot(dy - py) <= r),
                    "{x},{y}"
                );
            }
        }
    }
    // Nothing in the middle of the gap.
    assert!(
        dabs.iter().all(|&(x, _)| !(45.0..75.0).contains(&x)),
        "{dabs:?}"
    );
}

#[test]
fn copy_is_moves_clone_alone_and_leaves_the_object() {
    let (w, h) = (200, 120);
    let mut mask = vec![false; w * h];
    for y in 40..80 {
        for x in 30..70 {
            mask[y * w + x] = true;
        }
    }
    let sel = Selection {
        mask: mask.clone(),
        width: w,
        height: h,
        share: 1.0,
        selected: true,
        lo: (30, 40),
        hi: (69, 79),
        object: mask,
        shadow: Vec::new(),
        with_shadow: false,
    };
    let by = [0.5, 0.0];
    let moved = sel.move_by(by);
    let copied = sel.copy_by(by);
    // Move clones it there and removes it here; Copy only clones.
    assert_eq!(moved.len(), 2);
    assert_eq!(copied.len(), 1);
    assert!(matches!(&copied[0], Command::AddSpot { kind: Some(k), .. } if k == "clone"));
    assert_eq!(format!("{:?}", copied[0]), format!("{:?}", moved[0]));
}

#[test]
fn mask_dabs_fill_a_shape_with_few_big_dabs_and_stay_inside() {
    let (w, h) = (200, 120);
    let mut mask = vec![false; w * h];
    for y in 20..100 {
        for x in 30..170 {
            mask[y * w + x] = true;
        }
    }
    let sel = Selection {
        mask: mask.clone(),
        width: w,
        height: h,
        share: 1.0,
        selected: true,
        lo: (30, 20),
        hi: (169, 99),
        object: mask.clone(),
        shadow: Vec::new(),
        with_shadow: false,
    };
    let dabs = sel.mask_dabs();
    let px: Vec<(f64, f64, f64)> = dabs
        .iter()
        .map(|d| (d[0] * w as f64, d[1] * h as f64, d[2] * w as f64))
        .collect();
    // Every pixel of the shape is under a dab.
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                assert!(
                    px.iter()
                        .any(|&(cx, cy, r)| (cx - fx).hypot(cy - fy) <= r + 0.5),
                    "{x},{y}"
                );
            }
        }
    }
    // Not far outside, and far fewer dabs than pixels.
    for &(cx, cy, r) in &px {
        assert!(
            cx - r > 27.0 && cx + r < 173.0 && cy - r > 17.0 && cy + r < 103.0,
            "{cx},{cy} r {r}"
        );
    }
    assert!(px.len() < 600, "{} dabs", px.len());
}

#[test]
fn a_shadow_joined_to_the_object_is_found_and_a_dark_patch_away_from_it_is_not() {
    // A bright disc on mid grey, a dark ellipse under it, touching it,
    // and a dark square far off.
    let (w, h) = (400, 300);
    let mut rgb = vec![0.6f32; w * h * 3];
    let mut object = vec![false; w * h];
    let mut truth = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64, y as f64);
            let i = y * w + x;
            let disc = (fx - 200.0).hypot(fy - 120.0) < 60.0;
            let under = ((fx - 200.0) / 70.0).hypot((fy - 185.0) / 12.0) < 1.0;
            let patch = (20..60).contains(&x) && (20..60).contains(&y);
            let v = if disc {
                0.9
            } else if under || patch {
                0.25
            } else {
                0.6
            };
            rgb[i * 3..][..3].fill(v);
            object[i] = disc;
            truth[i] = under && !disc;
        }
    }
    let (lo, hi) = bounds(&object, w);
    let size = (hi.0 - lo.0).max(hi.1 - lo.1) as f64;
    let shadow = shadow_of(&object, &rgb, w, h, size);
    assert!(!shadow.is_empty());
    let found = (0..w * h).filter(|&i| shadow[i] && truth[i]).count();
    let want = truth.iter().filter(|t| **t).count();
    assert!(found * 10 > want * 7, "{found} of {want}");
    let stray = (0..w * h).filter(|&i| shadow[i] && !truth[i]).count();
    assert!(stray * 10 < want, "{stray} stray");
    assert!((0..60 * w).all(|i| !shadow[i] || i % w > 60));
    // On a dark background there's no shadow to take.
    let mut dark = rgb.clone();
    for (i, o) in object.iter().enumerate() {
        if !o {
            dark[i * 3..][..3].fill(0.1);
        }
    }
    assert!(shadow_of(&object, &dark, w, h, size).is_empty());
}

// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb cleanup`: the photos you may not want, each with why
//! (`amb_ops::cleanup`); `--trash` moves them to the trash, `--reject`
//! rejects them.

use amb_ops::cleanup::{self, Options as Find};
use amb_protocol::sidecar::Flag;
use std::path::Path;

use crate::organise::{Options, open_catalog, photos, read_info, write_info};
use crate::print_json;

const TESTS: [&str; 6] = ["dupes", "bursts", "blurry", "eyes", "exposure", "rejected"];

pub(crate) fn cleanup(
    args: &[&str],
    options: &Options,
    dry_run: bool,
    json: bool,
) -> Result<(), String> {
    let photos = photos(args)?;
    let cat = open_catalog(options)?;
    let skip: Vec<&str> = options
        .get("--skip")
        .map(|s| s.split(',').map(str::trim).collect())
        .unwrap_or_default();
    if let Some(bad) = skip.iter().find(|s| !TESTS.contains(s)) {
        return Err(format!("--skip {bad}: one of {}", TESTS.join(", ")));
    }
    let on = |t: &str| !skip.contains(&t);
    let default = Find::default();
    let find = Find {
        duplicates: on("dupes"),
        alike_bits: match options.get("--alike") {
            Some(b) => b.parse().map_err(|_| format!("--alike {b}: 0 to 20"))?,
            None => default.alike_bits,
        },
        burst_seconds: match (on("bursts"), options.get("--burst")) {
            (false, _) => 0.0,
            (true, Some(s)) => s.parse().map_err(|_| format!("--burst {s}: seconds"))?,
            (true, None) => default.burst_seconds,
        },
        blurry: on("blurry"),
        eyes: on("eyes"),
        exposure: on("exposure"),
        rejected: on("rejected"),
    };
    let small = |p: &Path| amb_engine::raw::embedded_thumbnail(p, 256).ok().flatten();
    let big = |p: &Path| amb_engine::raw::embedded_thumbnail(p, 1280).ok().flatten();
    let found = cleanup::find(&cat, &photos, find, &small, &big, &|done, total| {
        if !json && (done % 50 == 0 || done == total) {
            eprint!("\rLooking… {done} of {total}");
            if done == total {
                eprintln!();
            }
        }
    });
    let paths: Vec<_> = found.iter().map(|s| s.path.clone()).collect();
    let (trash, reject) = (
        options.contains_key("--trash"),
        options.contains_key("--reject"),
    );
    if trash && reject {
        return Err("--trash or --reject, not both".into());
    }
    if !dry_run && reject {
        for p in &paths {
            let mut info = read_info(p);
            info.flag = Some(Flag::Reject);
            write_info(p, &info)?;
        }
    }
    if !dry_run && trash && !paths.is_empty() {
        amb_library::files::trash(&paths)?;
        cat.forget(&paths);
    }
    if json {
        return print_json(&serde_json::to_value(&found).map_err(|e| e.to_string())?);
    }
    for s in &found {
        let why: Vec<String> = s.reasons.iter().map(|r| r.text()).collect();
        let keep = s
            .reasons
            .iter()
            .find_map(|r| r.keep())
            .map(|k| {
                format!(
                    " (keep {})",
                    k.file_name().unwrap_or_default().to_string_lossy()
                )
            })
            .unwrap_or_default();
        println!("{}: {}{keep}", s.path.display(), why.join(", "));
    }
    let n = found.len();
    let photos = format!("{n} photo{}", if n == 1 { "" } else { "s" });
    eprintln!(
        "{photos} you may not want{}",
        match (trash, reject, dry_run) {
            (true, _, false) => "; moved to the trash",
            (_, true, false) => "; rejected",
            (true, _, true) => "; would move them to the trash",
            (_, true, true) => "; would reject them",
            _ => " (photos you picked or rated are left out)",
        }
    );
    Ok(())
}

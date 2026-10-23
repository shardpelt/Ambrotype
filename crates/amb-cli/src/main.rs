// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb`: Ambrotype command-line tool.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod edit;
mod lens;
mod library;
mod look;
mod mcp;
mod models;
mod negative;
mod organise;
mod plugin;
mod presets;
mod render;
mod sweep;

use amb_presets::Preset;
use amb_presets::registry::Registry;
use amb_presets::report::{self, Report};

use edit::*;
use lens::*;
use library::*;
use look::*;
use models::*;
use negative::*;
use presets::*;
use render::*;
use sweep::*;

const USAGE: &str = "\
amb: Ambrotype command-line tool
Copyright (C) 2026 The Ambrotype contributors. Free software under the GNU GPL,
version 3 or later, with ABSOLUTELY NO WARRANTY: https://github.com/shardpelt/Ambrotype

USAGE:
    amb preset inspect <FILE>... [--json]   Show what a preset contains and how it maps to our support
    amb preset dump <FILE>                  Print the parsed settings as JSON
    amb preset census <PATH>... [--json]    Count tag usage across a preset collection (dirs are scanned recursively)
    amb sweep plan [--groups G,..]          List the Lightroom calibration sweeps
    amb sweep generate --out <DIR> [--groups G,..] [--synthetic P,..] [<RAW>...]
                                            Write a sweep kit for importing into Lightroom
                                            (docs/lightroom-trial-plan.md): raw copies + XMP sidecars,
                                            and/or synthetic DNG targets with embedded settings
    amb calibration defaults                Print the default model constants as calibration JSON
    amb testimage <PATTERN> <OUT.dng>       Write one synthetic target (ramp, hue, edges, colorchecker,
                                            negative: a scanned colour negative)
    amb render <RAW> <OUT.jpg|.png|.tif|.webp|.avif|.exr|.jxl> [--preset P] [--amount A]
                                            [--look default|linear] [--calibration fitted.json]
                                            [--lens-db DIR] [--lcp-dir DIR] [export options]
                                            Develop a raw with its own sidecar (or --preset, a preset or
                                            another sidecar), export it, and report which settings are not
                                            rendered yet
    amb render <RAW>... --out-dir DIR [--format jpeg|...] [--name TEMPLATE] [same options]
                                            The same for many raws (e.g. a folder: photos/*.NEF), with the
                                            same preset and export options; one failure doesn't stop the rest.
                                            --name names the files as Import does: {name} {yyyy} {mm} {dd}
                                            {hh} {min} {ss} {seq} {camera}, e.g. trip_{seq}
    amb do <PHOTO|SIDECAR.xmp> \"<EDITS>\" [--dry-run] [--json]
                                            Edit in the compact syntax as one undoable step and print
                                            only what changed, e.g. \"exp=+0.5 con+=10 hl=-40 reset:hsl
                                            preset:film.xmp@50\" (Name=v, Name+=d, Name-=d, reset:X,
                                            preset:P[@amount], snap:N, load:N, undo, redo; short names
                                            temp tint exp con hl sh wh bl tex cla deh vib sat sharp nr
                                            cnr vig grain; docs/ai/core.md)
    amb edit <SIDECAR.xmp> --commands <CMDS.json|-> [--out <NEW.xmp|.json>] [--dry-run] [--brief] [--json]
                                            Apply protocol commands (set_param, adjust_param, reset_param,
                                            reset_group, apply_preset, add_mask, set_mask, paint_mask,
                                            remove_mask, duplicate_mask, move_mask, add_spot, set_spot, remove_spot, undo, redo)
                                            as one transaction,
                                            print the diff and write the sidecar (other XMP metadata kept)
    amb match <PHOTO> <REF>... [--apply] [--parts wb,tone,color,grading,hsl] [--out PRESET.xmp] [--json]
                                            Find the settings that make the photo look like the reference
                                            images (JPEGs, or raws with their edits): white balance, tone,
                                            Vibrance/Saturation, Color Grading tints and the Color Mixer
                                            (HSL per colour), solved on small
                                            renders. Shows the changes; --apply makes them one undoable
                                            step, --out also writes them as a preset
    amb auto <PHOTO> [--tone] [--wb] [--apply] [--out PRESET.xmp] [--json]
                                            Lightroom-style Auto: the tone sliders (and Vibrance for dull
                                            colour) and the white balance (as Auto), from the photo's own
                                            look; --tone or --wb for one of them. Shows the changes;
                                            --apply makes them one undoable step
    amb stats <PHOTO> [--preset P] [--amount A] [--json]
                                            The photo developed small (with its edit, or a preset) as a
                                            few numbers instead of a picture: L* percentiles, clipped
                                            shadows and highlights, midtone colour cast, chroma
    amb query <SIDECAR.xmp> --queries <QUERIES.json|->
                                            Answer protocol queries (get_param, list_params, describe,
                                            list_groups, list_masks, list_spots) as JSON
    amb protocol schema [command|query]     Print the JSON Schema of commands and queries
    amb negative analyze <RAW>... [--slides] [--out ROLL.xmp] [--json]
                                            Estimate film base, levels and midtone of scanned negatives
                                            (the median over all frames, for a consistent roll) and write
                                            them as a preset with Auto off; --slides for slide film
    amb negative stocks [--json]            List the built-in film stock looks
    amb lens info <RAW> [--lens-db DIR] [--lcp-dir DIR]
                                            Show a raw's lens metadata and the profile it gets
    amb lens match --camera-make X --camera-model Y --lens-model M [--lens-make L] [--focal F]
                   [--aperture A] [--distance D] [--crop C] [--lens-db DIR] [--lcp-dir DIR]
                                            Find the lens profile for a shot (Adobe .lcp files you have,
                                            then the lensfun database; default locations or
                                            AMB_LCP_DIRS / AMB_LENSFUN_DB) and print the models
                                            interpolated to its focal length, aperture and distance
    amb lens fetch [--to DIR] [--from version_2.tar.bz2] [--force]
                                            Download lensfun's lens database (CC BY-SA 3.0) from its
                                            update server into ~/.local/share/lensfun/updates, where it is
                                            found by default; --from installs a downloaded copy
    amb models [list] [--json]              The AI models for subject, background and people masks: every
                                            freely licensed one we can install, its licence, size and
                                            speed, and which is in use (* in use)
    amb models install <ID>                 Download it (checked against its sha256) into the models folder
    amb models use <ID>                     Use it for its task's masks (it has to be installed)
    amb models add <FILE.onnx> --task subject|people --name NAME --size PX [--mean r,g,b]
                [--std r,g,b] [--sigmoid|--softmax] [--nhwc] [--channel N] [--invert]
                [--licence L] [--source URL]
                                            Your own ONNX model: a square RGB photo in, a map of the same
                                            side out (or one per class: --channel picks one, --softmax
                                            turns scores into shares, --invert flips it; --nhwc for
                                            TensorFlow/MediaPipe models); checked that it runs.
                                            Guide: docs/own-models.md
    amb models remove <ID>                  Delete it; its task goes back to its default
    amb import <SOURCE> <DEST> [--into \"{yyyy}/{yyyy}-{mm}-{dd}\"] [--rename \"{name}\"] [--start 1]
               [--keywords \"a, b\"] [--preset LOOK.xmp] [--backup DIR] [--no-subfolders] [--dry-run] [--json]
                                            Copy photos from a card or folder into DEST, sorted into
                                            folders and renamed from templates ({name} {yyyy} {yy} {mm}
                                            {dd} {hh} {min} {ss} {seq} {camera}); a raw's JPEG and .xmp
                                            keep its new name; photos already there are skipped. --preset
                                            gives photos without an edit of their own a look; --backup
                                            also copies the originals to DIR/Imported on YYYY-MM-DD
    amb collage list                        The built-in collage layouts
    amb collage <LAYOUT> <PHOTO>... --out OUT.jpg [--size WxH] [--background #fff] [--gap 1.5]
                [--margin 3] [export options]
                                            Several photos (each with its own edit) on one canvas, as
                                            the editor's collage maker does: LAYOUT is a built-in's
                                            name or number, or a layout .json. Photos fill the cells by
                                            shape; more photos than cells make pages, OUT-001.jpg, ...
    amb catalog scan <DIR>...               Catalogue the photos in DIR and its subfolders: what the
                                            library reads from each (rating, keywords, capture time,
                                            camera, lens, ...) is kept until the files change, so big
                                            folders open quickly. Deleting the catalogue loses nothing
    amb catalog find [--text T] [--camera C] [--lens L] [--keyword K] [--rating N]
                [--from DATE] [--until DATE] [--json]
                                            Search every catalogued photo, newest first
    amb catalog stats | forget <DIR>        How many photos it knows; forget a folder (files stay).
                                            --catalog FILE uses another catalogue than the editor's
    amb catalog add <DIR> | remove <DIR>    Library folders (whole drives too) the editor keeps indexed
    amb catalog index                       Bring every library folder up to date; an unplugged drive's
                                            photos stay listed, offline. --folders FILE: another list
    amb catalog folders [--json]            Each catalogued folder with its photo count
    amb catalog missing [--json]            Photos whose files are gone from a folder that can be reached
                                            (moved or deleted outside Ambrotype)
    amb catalog relink <DIR>                Find them in DIR and its subfolders by name and size; the
                                            catalogue and collections follow them there
    amb info <PHOTO|DIR>... [--json]        What each photo's sidecar says: rating, label, flag,
                                            keywords, title, caption, creator, copyright, place, stack
    amb tag <PHOTO|DIR>... [--rating 0-5] [--label Red|...|none] [--flag pick|reject|none]
            [--add \"a, b\"] [--remove \"c\"] [--title T] [--caption C] [--creator C]
            [--copyright C] [--place LAT,LON|none] [--dry-run] [--json]
                                            One change to many photos at once, in their sidecars, as the
                                            library does it (a place as Lightroom's map writes it)
    amb place <PHOTO|DIR>... --gpx TRACK.gpx [--offset HOURS] [--replace] [--dry-run] [--json]
                                            Where photos were taken, from a GPS track: --offset is how
                                            many hours the camera's clock was ahead of UTC; photos with a
                                            place (or the camera's GPS) keep it unless --replace
    amb duplicates <PHOTO|DIR>... [--alike 0-20] [--burst SECONDS] [--stack] [--reject] [--json]
                                            Same files, alike pictures (--alike bits of 64 may differ,
                                            default 6; 0: same files only) and bursts (default 2 s),
                                            best first; --stack stacks each group with the best on top,
                                            --reject picks the best and rejects the rest
    amb move <PHOTO|DIR>... --to DIR [--dry-run] [--json]
    amb rename <PHOTO|DIR>... --name TEMPLATE [--dry-run] [--json]
    amb sort <PHOTO|DIR>... --into DIR [--by \"{yyyy}/{yyyy}-{mm}-{dd}\"] [--dry-run] [--json]
                                            Move, rename (Import's tokens: {name} {yyyy} {mm} {dd} {seq}
                                            {camera} ...) or sort into dated folders, each photo with its
                                            sidecar, JPEG twin and virtual copies; nothing is overwritten
                                            (-2, -3, ...); the catalogue and collections follow
    amb trash <PHOTO|DIR>... [--dry-run]    Photos with all their files to the system's trash
    amb slideshow <PHOTO|DIR>... [--seconds 1-600] [--shuffle] [--window]
                  [--effect none|fade|slide|zoom|black|random]
                                            Opens the editor straight into a slideshow of the photos, each
                                            with its edit: 4 s each, a fade, full screen unless --window;
                                            random picks a change each time. Space pauses, Esc ends
    amb keywords [list] [--json] | rename <K>... --to <NEW> | remove <K>... [--dry-run]
                                            Every keyword in the catalogue with its count; rename, merge
                                            or remove keywords in every photo's sidecar
    amb collection [list] | show <NAME> | add <NAME> <PHOTO>... | remove <NAME> <PHOTO>...
                   | delete <NAME> [--collections DIR] [--json]
                                            The editor's collections (photos from any folders)
    amb lrcat import <CATALOG.lrcat> [--apply] [--remap OLD=NEW] [--collections DIR] [--json]
                                            Move from Lightroom Classic: each photo's develop settings,
                                            rating, label, flag, keywords, stack, title and caption go into
                                            its XMP sidecar, virtual copies become ours, and collections
                                            (in their sets) the editor's. Shows the plan; --apply writes it.
                                            The catalogue is only read (close Lightroom first); sidecars
                                            already there are kept. --remap /Volumes/Photos/=/mnt/photos/
                                            finds photos that have moved
    amb merge hdr <RAW>... [--out OUT.dng] [--no-align] [--deghost low|medium|high] [--json]
                                            Merge bracketed raws of one scene into a floating-point DNG
                                            (default: <first>-HDR.dng next to them) that opens and edits
                                            like a raw, here and in Lightroom; Auto Tone it with
                                            amb plugin run plugins/auto-tone OUT.dng
    amb merge pano <RAW>... [--out OUT.dng] [--projection auto|spherical|cylindrical|perspective]
                   [--no-crop] [--no-lens] [--lens-db PATH] [--lcp-dir DIR] [--json]
                                            Stitch overlapping raws (camera turned about the lens) into
                                            a panorama DNG (default: <first>-Pano.dng next to them),
                                            each corrected with its lens profile first; --no-crop keeps
                                            the uncovered edges (black)
    amb mcp                                 Serve the Model Context Protocol over stdio, so your own AI app
                                            (Claude Desktop, Claude Code, ...) can edit, preview and export
                                            photos with Ambrotype's commands (docs/ai/mcp.md)
    amb profile list <RAW> [--dcp-dir DIR]  List the camera profiles (.dcp) on this computer for a raw's
                                            camera (Camera Raw's profile folders, AMB_DCP_DIRS, --dcp-dir);
                                            a preset's or sidecar's CameraProfile picks one by name
    amb ref [GROUP]                         The core card for AI clients (docs/ai/core.md: the compact
                                            syntax and the most used settings), or one group's settings
                                            one line each
    amb settings reference [--json]         Print the settings reference (docs/ai/settings-reference.md)
    amb plugin new <DIR> [--name \"My plugin\"]
                                            Start a plugin project (Rust, built to WebAssembly;
                                            docs/ai/plugins.md)
    amb plugin build <DIR>                  Build it (needs: rustup target add wasm32-unknown-unknown),
                                            then check it
    amb plugin check <DIR>                  Validate plugin.json and the module, and run it on a sample
    amb plugin run <DIR> <PHOTO|SIDECAR.xmp> [--params a=1,b=Film] [--dry-run] [--json]
                                            Run a plugin on a photo's edit and write its sidecar

EXPORT OPTIONS (amb render):
    --format jpeg|png|tiff|webp|avif|exr|jxl  Default: from the file extension
    --quality 1-100                         JPEG/AVIF quality (default 90)
    --bits 8|16|32                          PNG 8/16, TIFF 8/16/32 (float); default 8, TIFF 16
    --color-space srgb|p3|adobergb|prophoto|rec2020   Default srgb; an ICC profile is embedded
    --long-edge N | --short-edge N | --width N | --height N | --fit WxH | --megapixels M | --percent P
    --exact WxH                             Exactly W×H (e.g. 1080x1350): centre-cropped if the aspect differs
    --enlarge                              Allow upscaling (default: never enlarge)
    --sharpen screen|matte|glossy[:low|standard|high]   Output sharpening
    --metadata all|no-location|copyright|none           EXIF to include (default no-location)
    --max-size 500k|2M                      JPEG: lower the quality until the file fits
    --subsampling 444|422|420  --progressive  --avif-speed 1-10
    --rating 1-5  --keywords \"beach, sunset\"   Put a rating and keywords in the file (XMP, IPTC in JPEG,
                                            EXIF), with the all / no-location metadata policies
    --border 5%|40px[,#ffffff|auto] | border.json   A frame around the photo; a .json preset can add
                                            a deeper bottom, a keyline, rounded corners, and fit (with
                                            --exact: the whole photo inside the frame, not cropped)
    --border-caption \"{camera} · {exposure}\"   A line of text in the border's bottom (made three
                                            times the width deep if not set): {camera} {lens}
                                            {exposure} {focal} {aperture} {shutter} {iso} {date}
                                            {year} {title} {creator} {copyright}
    --watermark \"© Name\" | logo.png | watermark.json   Text or a logo (PNG, its transparency kept) on
                                            the photo; --watermark-at bottom-right (top-left, top, top-right,
                                            left, center, right, bottom-left, bottom), --watermark-size 20
                                            (% of the width), --watermark-opacity 70, --watermark-rotate 30
                                            (degrees), --watermark-tile (repeat it across the photo)

Supported inputs: .xmp presets and sidecars, .lrtemplate presets, and files with an embedded
XMP packet (e.g. .dng presets for Lightroom mobile). AI and script guide: docs/ai/README.md.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Effect plugins in edits render as in the editor: built in, or from its
    // plugins folder.
    let plugins =
        amb_library::collections::editor_dir().and_then(|d| Some(d.parent()?.join("plugins")));
    if let Some(models) = amb_ai::default_dir() {
        amb_engine::subject::set_finder(amb_ai::finder(models));
    }
    amb_engine::plugin_effects::set_resolver(amb_plugin::effect::resolver(move |id| {
        amb_plugin::find(plugins.as_deref(), id)
    }));
    let mut json = false;
    let mut dry_run = false;
    let mut brief = false;
    let mut options: BTreeMap<&str, &str> = BTreeMap::new();
    let mut positional: Vec<&str> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--json" => json = true,
            "--dry-run" => dry_run = true,
            "--brief" => brief = true,
            "--progressive" | "--enlarge" | "--slides" | "--force" | "--no-subfolders"
            | "--no-align" | "--no-crop" | "--no-lens" | "--apply" | "--tone" | "--wb"
            | "--watermark-tile" | "--sigmoid" | "--softmax" | "--nhwc" | "--invert"
            | "--replace" | "--stack" | "--reject" | "--shuffle" | "--window" => {
                options.insert(arg.as_str(), "true");
            }
            "--out"
            | "--groups"
            | "--synthetic"
            | "--preset"
            | "--amount"
            | "--look"
            | "--calibration"
            | "--commands"
            | "--queries"
            | "--camera-make"
            | "--camera-model"
            | "--lens-make"
            | "--lens-model"
            | "--focal"
            | "--aperture"
            | "--distance"
            | "--crop"
            | "--lens-db"
            | "--lcp-dir"
            | "--format"
            | "--quality"
            | "--bits"
            | "--color-space"
            | "--subsampling"
            | "--long-edge"
            | "--short-edge"
            | "--width"
            | "--height"
            | "--fit"
            | "--exact"
            | "--megapixels"
            | "--percent"
            | "--sharpen"
            | "--metadata"
            | "--max-size"
            | "--avif-speed"
            | "--border"
            | "--border-caption"
            | "--out-dir"
            | "--watermark"
            | "--watermark-at"
            | "--watermark-size"
            | "--watermark-opacity"
            | "--watermark-rotate"
            | "--rating"
            | "--keywords"
            | "--name"
            | "--params"
            | "--into"
            | "--rename"
            | "--start"
            | "--to"
            | "--from"
            | "--dcp-dir"
            | "--deghost"
            | "--projection"
            | "--parts"
            | "--remap"
            | "--backup"
            | "--size"
            | "--background"
            | "--gap"
            | "--margin"
            | "--collections"
            | "--camera"
            | "--lens"
            | "--keyword"
            | "--text"
            | "--until"
            | "--catalog"
            | "--folders"
            | "--task"
            | "--channel"
            | "--mean"
            | "--std"
            | "--licence"
            | "--source"
            | "--label"
            | "--flag"
            | "--add"
            | "--remove"
            | "--title"
            | "--caption"
            | "--creator"
            | "--copyright"
            | "--place"
            | "--gpx"
            | "--offset"
            | "--alike"
            | "--burst"
            | "--by"
            | "--seconds"
            | "--effect" => match iter.next() {
                Some(value) => {
                    options.insert(arg.as_str(), value.as_str());
                }
                None => {
                    eprintln!("error: {arg} needs a value\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            other if other.starts_with("--") => {
                eprintln!("error: unknown option {other}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            other => positional.push(other),
        }
    }
    let groups = options
        .get("--groups")
        .map(|g| g.split(',').map(str::trim).collect::<Vec<_>>());
    let result = match positional.as_slice() {
        ["preset", "inspect", files @ ..] if !files.is_empty() => inspect(files, json),
        ["preset", "dump", file] => dump(file),
        ["preset", "census", paths @ ..] if !paths.is_empty() => census(paths, json),
        ["sweep", "plan"] => sweep_plan(groups.as_deref()),
        ["sweep", "generate", raws @ ..] => match options.get("--out") {
            Some(out) => sweep_generate(
                raws,
                options.get("--synthetic").copied(),
                Path::new(out),
                groups.as_deref(),
            ),
            None => Err("sweep generate needs --out <DIR>".into()),
        },
        ["testimage", pattern, out] => testimage(pattern, Path::new(out)),
        ["profile", "list", raw] => profile_list(Path::new(raw), &options),
        ["mcp"] => mcp::serve(),
        ["collage", "list"] => {
            for (i, t) in amb_export::collage::builtin_templates().iter().enumerate() {
                println!("{:>2}  {} ({} photos)", i + 1, t.name, t.cells.len());
            }
            Ok(())
        }
        ["collage", layout, photos @ ..] if !photos.is_empty() => match options.get("--out") {
            Some(out) => collage(layout, photos, Path::new(out), &options),
            None => Err("collage needs --out <FILE>".into()),
        },
        ["lrcat", "import", catalog] => lrcat_import(Path::new(catalog), &options, json),
        ["catalog", "missing"] => organise::missing(None, &options, json),
        ["catalog", "relink", dir] => organise::missing(Some(dir), &options, json),
        ["catalog", rest @ ..] => catalog(rest, &options, json),
        ["info", photos @ ..] if !photos.is_empty() => organise::info(photos, json),
        ["tag", photos @ ..] if !photos.is_empty() => {
            organise::tag(photos, &options, dry_run, json)
        }
        ["place", photos @ ..] if !photos.is_empty() => {
            organise::place(photos, &options, dry_run, json)
        }
        ["duplicates", photos @ ..] if !photos.is_empty() => {
            organise::duplicates(photos, &options, json)
        }
        [kind @ ("move" | "rename" | "sort"), photos @ ..] if !photos.is_empty() => {
            organise::relocate(kind, photos, &options, dry_run, json)
        }
        ["trash", photos @ ..] if !photos.is_empty() => organise::trash(photos, &options, dry_run),
        ["slideshow", photos @ ..] if !photos.is_empty() => {
            organise::slideshow(photos, &options, dry_run)
        }
        ["keywords", rest @ ..] => organise::keywords(rest, &options, dry_run, json),
        ["collection", rest @ ..] => organise::collection(rest, &options, json),
        ["merge", "hdr", raws @ ..] if raws.len() >= 2 => merge_hdr(raws, &options, json),
        ["merge", "pano", raws @ ..] if raws.len() >= 2 => merge_pano(raws, &options, json),
        ["import", source, dest] => {
            import(Path::new(source), Path::new(dest), &options, dry_run, json)
        }
        ["calibration", "defaults"] => {
            println!(
                "{}",
                amb_engine::calibration::Calibration::default().to_json()
            );
            Ok(())
        }
        ["render", inputs @ ..] if !inputs.is_empty() && options.contains_key("--out-dir") => {
            render_many(inputs, Path::new(options["--out-dir"]), &options)
        }
        ["render", input, out] => render(
            Path::new(input),
            Path::new(out),
            options.get("--preset").copied(),
            options.get("--amount").copied(),
            options.get("--look").copied(),
            options.get("--calibration").copied(),
            &options,
        ),
        ["edit", sidecar] => match options.get("--commands") {
            Some(commands) => edit(
                Path::new(sidecar),
                commands,
                options.get("--out").map(Path::new),
                dry_run,
                if json {
                    Output::Json
                } else if brief {
                    Output::Brief
                } else {
                    Output::Text
                },
            ),
            None => Err("edit needs --commands <file.json> (or - for stdin)".into()),
        },
        ["do", target, edits @ ..] if !edits.is_empty() => {
            do_edits(Path::new(target), &edits.join(" "), dry_run, json)
                .map(|text| println!("{text}"))
        }
        ["match", photo, refs @ ..] if !refs.is_empty() => match_look(
            Path::new(photo),
            refs,
            &options,
            options.contains_key("--apply"),
            json,
        )
        .map(|t| println!("{t}")),
        ["auto", photo] => auto(
            Path::new(photo),
            &options,
            options.contains_key("--apply"),
            json,
        )
        .map(|t| println!("{t}")),
        ["stats", photo] => stats(Path::new(photo), &options, json).map(|t| println!("{t}")),
        ["query", state] => match options.get("--queries") {
            Some(queries) => query(Path::new(state), queries),
            None => Err("query needs --queries <file.json> (or - for stdin)".into()),
        },
        ["negative", "analyze", raws @ ..] if !raws.is_empty() => negative_analyze(
            raws,
            options.get("--out").map(Path::new),
            options.contains_key("--slides"),
            json,
        ),
        ["negative", "stocks"] => {
            negative_stocks(json);
            Ok(())
        }
        ["lens", "info", raw] => lens_info(Path::new(raw), &options).map(|text| println!("{text}")),
        ["lens", "match"] => lens_match_output(&options).map(|text| println!("{text}")),
        ["lens", "fetch"] => lens_fetch(&options),
        ["models"] | ["models", "list"] => models_list(json),
        ["models", "install", id] => models_install(id),
        ["models", "use", id] => models_use(id),
        ["models", "remove", id] => models_remove(id),
        ["models", "add", file] => models_add(Path::new(file), &options),
        ["protocol", "schema"] => print_json(&amb_protocol::schema::protocol_schema()),
        ["protocol", "schema", "command"] => print_json(&amb_protocol::schema::command_schema()),
        ["protocol", "schema", "query"] => print_json(&amb_protocol::schema::query_schema()),
        ["plugin", "new", dir] => plugin::new(Path::new(dir), options.get("--name").copied()),
        ["plugin", "build", dir] => plugin::build(Path::new(dir)),
        ["plugin", "check", dir] => plugin::check(Path::new(dir)),
        ["plugin", "run", dir, target] => plugin::run(
            Path::new(dir),
            Path::new(target),
            options.get("--params").copied(),
            dry_run,
            json,
        )
        .map(|text| println!("{text}")),
        ["ref"] => {
            print!("{}", amb_protocol::card::core());
            Ok(())
        }
        ["ref", group] => match amb_protocol::card::group(group) {
            Some(page) => {
                print!("{page}");
                Ok(())
            }
            None => Err(format!(
                "no group {group:?}; groups: {}",
                amb_protocol::catalog::Catalog::builtin()
                    .groups()
                    .iter()
                    .map(|g| g.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        },
        ["settings", "reference"] => {
            if json {
                print_json(&amb_protocol::reference::json())
            } else {
                print!("{}", amb_protocol::reference::markdown());
                Ok(())
            }
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn print_json(value: &serde_json::Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_command_end_to_end() {
        let dir = std::env::temp_dir().join(format!("amb-cli-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("photo.xmp");
        std::fs::write(
            &sidecar,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmp:Rating="5" crs:Exposure2012="+0.30"/></rdf:RDF></x:xmpmeta>"#,
        )
        .unwrap();
        let cmds = r#"[{"cmd":"adjust_param","name":"Exposure2012","delta":0.2},
                       {"cmd":"set_param","name":"Vibrance","value":150}]"#;
        let text = edit_output(&sidecar, cmds, None, false, Output::Text).unwrap();
        assert!(text.contains("Exposure2012: +0.3 -> +0.5"), "{text}");
        assert!(text.contains("clamped: Vibrance = 150"), "{text}");
        let written = std::fs::read_to_string(&sidecar).unwrap();
        assert!(written.contains(r#"crs:Exposure2012="+0.5""#), "{written}");
        assert!(written.contains(r#"xmp:Rating="5""#), "{written}");

        let json = edit_output(
            &sidecar,
            r#"{"cmd":"reset_param","name":"Vibrance"}"#,
            None,
            true,
            Output::Json,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["dry_run"], serde_json::json!(true));
        assert_eq!(
            v["results"][0]["diff"][0]["before"],
            serde_json::json!(100.0)
        );

        let err = edit_output(
            &sidecar,
            r#"{"cmd":"set_param","name":"Exposur","value":1}"#,
            None,
            false,
            Output::Text,
        )
        .unwrap_err();
        assert!(err.contains("did you mean Exposure2012"), "{err}");

        // `amb do` on the photo finds its sidecar, and reports briefly.
        let photo = dir.join("photo.nef");
        let text = do_edits(&photo, "exp+=0.25 vib=10", false, false).unwrap();
        assert_eq!(
            text,
            "Exposure2012 +0.5→+0.75\nVibrance +100→+10\nsaved photo.xmp"
        );
        let text = do_edits(&sidecar, "undo", true, false).unwrap();
        assert!(text.ends_with("dry run, not saved"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lens_match_command() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../amb-lens/tests/fixtures");
        let lens_db = fixtures.join("lensfun").display().to_string();
        let lcp_dir = fixtures.join("lcp").display().to_string();
        let empty_dir = fixtures.join("lensfun").display().to_string();
        let mut options: BTreeMap<&str, &str> = BTreeMap::from([
            ("--camera-make", "Acme"),
            ("--camera-model", "Mini 2"),
            ("--lens-make", "Acme"),
            ("--lens-model", "Zoomar 24-70mm F2.8 Pro | A"),
            ("--focal", "47"),
            ("--aperture", "4"),
            ("--lens-db", lens_db.as_str()),
            ("--lcp-dir", lcp_dir.as_str()),
        ]);
        let text = lens_match_output(&options).unwrap();
        assert!(
            text.contains("crop 1.5 (lensfun camera database)"),
            "{text}"
        );
        assert!(
            text.contains("profile: Lcp | LCP: Adobe (Acme Zoomar"),
            "{text}"
        );
        assert!(
            text.contains(
                "lensfun  Acme Zoomar 24-70mm f/2.8 Pro | A (score 1.000, calibration crop 1.5)"
            ),
            "{text}"
        );
        assert!(text.contains("distortion  acm k=["), "{text}");

        // Without LCP profiles the lensfun calibration for the APS-C body is used.
        options.insert("--lcp-dir", empty_dir.as_str());
        let text = lens_match_output(&options).unwrap();
        assert!(text.contains("profile: Lensfun | lensfun: Acme Zoomar 24-70mm f/2.8 Pro | A (calibrated at crop 1.5)"), "{text}");
        // 47 mm is halfway between a poly3 (24 mm) and a ptlens (70 mm) calibration.
        assert!(
            text.contains("distortion  ptlens a=0.001 b=0.002 c=0.003"),
            "{text}"
        );
        assert!(
            text.contains("tca         linear kr=1.002 kb=0.9985"),
            "{text}"
        );

        options.insert("--focal", "wide");
        assert!(
            lens_match_output(&options)
                .unwrap_err()
                .contains("bad --focal")
        );
        options.remove("--lens-model");
        assert!(
            lens_match_output(&options)
                .unwrap_err()
                .contains("--lens-model")
        );
    }
}

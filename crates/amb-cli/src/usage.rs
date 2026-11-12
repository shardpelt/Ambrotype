// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb`'s help text: `amb --help`, and the man page made from it (`man.rs`).

pub(crate) const USAGE: &str = "\
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
                                            negative: a scanned colour negative, dust: a slide with dust)
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
    amb render <PHOTO|DIR>... --out-dir DIR [--recursive] [--flat] [--skip-existing] [--dry-run] [--json]
                                            [export options]
                                            Smaller copies of folders full of photos (raws, JPEGs, ...):
                                            with --recursive the subfolders' too, in the same folders under
                                            DIR (--flat: all in DIR; clashing names get -2, -3). A raw with a
                                            JPEG of the same name counts once, as the raw. A few at a time;
                                            --skip-existing leaves files that are newer than their photo and
                                            its edit, so a stopped run carries on (change DIR when you change
                                            the size). Ends with a summary; --dry-run lists the files
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
    amb fuji <RECIPE.txt|-> [PHOTO] [--sensor S] [--name N] [--out PRESET.xmp] [--apply] [--json]
                                            A Fujifilm-style camera recipe, pasted as it's written (Film
                                            Simulation, Dynamic Range, Highlight, Shadow, Color, Grain Effect,
                                            White Balance with its shift…), recreated as develop settings
                                            through the Film recipe plugin. --sensor (X-Trans I to V, Bayer,
                                            GFX, or a camera model such as X-T3) keeps the settings to what
                                            those cameras have, when the recipe doesn't say. --out writes it
                                            as a preset in the Fuji group; with a PHOTO, --apply puts it on
                                            the photo (one undoable step)
    amb convert <PHOTO|DIR>... --to FOLDER [--look camera|auto|like:PHOTO] [--no-edits] [--best-of-bursts]
                [export options]
                                            Raws to JPEGs in one go (a card shot in RAW by mistake): camera,
                                            the default, gives what the camera would have saved (its own
                                            JPEG inside the raw when that is full size, else the raw matched
                                            to it); auto, each photo's Auto tone and white balance; like:PHOTO,
                                            each matched to the look of one you edited. Auto and like keep
                                            their settings in the raws' sidecars (not with --no-edits).
                                            JPEGs already there are kept, so a stopped run carries on.
                                            --best-of-bursts converts only the best photo of each burst
    amb replay <PHOTO> --out REPLAY.mp4 [--long-edge N] [--no-captions]
                                            Edit Replay: a short video of the photo's edit from its History,
                                            each step easing in and named, the before and the after held;
                                            AV1 in MP4, 1080 pixels on the long side by default
    amb contact <PHOTO|DIR>... --out SHEET.jpg [--style film|plain] [--columns N]
                [--paper a4|a3|letter|8x10|11x14] [--landscape] [--title TEXT]
                                            Contact sheets at 300 dpi: each photo small with its edit, as
                                            strips of 35 mm film (sprocket holes, frame numbers and the
                                            title as edge print; 6 frames a strip, a 36-frame roll on one
                                            A4 page) or a plain grid with file names. More photos than
                                            fit make pages: SHEET-001.jpg, ...
    amb search <WORDS> <PHOTO|DIR>... [--json]
    amb search --like <PHOTO> <PHOTO|DIR>... [--json]
                                            Smart search: the photos ranked by what is in them (dog on a
                                            beach, snowy mountains), or by likeness to another photo,
                                            with the chosen search model (`amb models install clip-b32`,
                                            156 MB, or siglip2-base, 690 MB and better). Photos are indexed
                                            once (about 0.7 s each) and kept in the catalogue
    amb share <PHOTO> --out PAGE.html [--long-edge 2048]
                                            One web page with the photo before its edit (framed as the
                                            edit is) and after, and a bar to drag between them; both
                                            photos are inside the file, so it works offline and on any
                                            web host
    amb dust <PHOTO> [--line X0,Y0,X1,Y1] [--apply] [--json]
                                            Dust and hairs on a film scan (darker specks and short curved
                                            lines on smooth ground; not long straight scratches), each as a
                                            heal spot with its donor. Shows them; --apply adds them as one
                                            undoable step. Spots already there are kept, and a second run
                                            adds only new ones. --line heals a straight scratch along a line
                                            (photo fractions, roughly drawn: it moves onto the scratch)
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
                                            keywords, title, caption, creator, copyright, place, stack,
                                            orientation
    amb tag <PHOTO|DIR>... [--rating 0-5] [--label Red|...|none] [--flag pick|reject|none]
            [--add \"a, b\"] [--remove \"c\"] [--title T] [--caption C] [--creator C]
            [--copyright C] [--place LAT,LON|none] [--rotate left|right|180]
            [--flip horizontal] [--dry-run] [--json]
                                            One change to many photos at once, in their sidecars, as the
                                            library does it (a place as Lightroom's map writes it, a turn
                                            as tiff:Orientation)
    amb place <PHOTO|DIR>... --gpx TRACK.gpx [--offset HOURS] [--replace] [--dry-run] [--json]
                                            Where photos were taken, from a GPS track: --offset is how
                                            many hours the camera's clock was ahead of UTC; photos with a
                                            place (or the camera's GPS) keep it unless --replace
    amb place <PHOTO|DIR>... --guess [--within MINUTES] [--clock HOURS] [--dry-run] [--json]
                                            Guess places for photos without one from the photos that have
                                            one: taken within MINUTES (default 30; between two placed
                                            photos, on the line between them), or looking the same (photos
                                            Smart search has indexed). --clock: hours the unplaced photos'
                                            clock was behind the placed ones'
    amb insights <PHOTO|DIR>... [--by aperture,focal,shutter,iso,hour,camera,lens] [--json]
                                            How the photos were shot: how often each aperture, focal
                                            length, shutter speed, ISO, hour, camera and lens comes up,
                                            and how many of each were kept (picked or 3 stars and up);
                                            then what the numbers suggest and challenges to try
    amb cleanup <PHOTO|DIR>... [--skip dupes,bursts,blurry,eyes,exposure,rejected] [--alike 0-20]
                [--burst SECONDS] [--trash|--reject] [--dry-run] [--json]
                                            The photos you may not want, each with why: copies and alike
                                            pictures (all but the best), the weaker frames of bursts,
                                            blurry, eyes closed, nearly black or blown out, rejected;
                                            photos picked or rated are left out. --trash moves them to
                                            the trash, --reject rejects them
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
    amb slideshow <PHOTO|DIR>... [--seconds 1-600] [--shuffle] [--window] [--subfolders]
                  [--effect none|fade|slide|zoom|black|random]
                                            Opens the editor straight into a slideshow of the photos, each
                                            with its edit: 4 s each, a fade, full screen unless --window;
                                            random picks a change each time; --subfolders takes in every
                                            folder under them. Space pauses, Esc ends
    amb keywords [list] [--json] | rename <K>... --to <NEW> | remove <K>... [--dry-run]
                                            Every keyword in the catalogue with its count; rename, merge
                                            or remove keywords in every photo's sidecar
    amb collection [list] | show <NAME> | add <NAME> <PHOTO>... | remove <NAME> <PHOTO>...
                   | delete <NAME> [--collections DIR] [--json]
                                            The editor's collections (photos from any folders)
    amb lrcat open <CATALOG.lrcat> | close <CATALOG.lrcat> | list [--json]
    amb lrcat sample <OUT.lrcat> <PHOTO>...  A small catalogue in Lightroom's layout of these photos (rated
                                            4, Red, picked, Exposure +1, a collection Selects), to try
                                            catalogues without Lightroom
                                            Open a Lightroom Classic catalogue in place: nothing imported
                                            or written. The editor's library and every amb command read
                                            its photos' edits, ratings and keywords from it (a sidecar
                                            written since wins); its folders and collections show in the
                                            library's Folders panel
    amb lrcat import <CATALOG.lrcat> [--apply] [--remap OLD=NEW] [--collections DIR] [--json]
                                            Move from Lightroom Classic: each photo's develop settings,
                                            rating, label, flag, keywords, stack, title and caption go into
                                            its XMP sidecar, virtual copies become ours, and collections
                                            (in their sets) the editor's. Shows the plan; --apply writes it.
                                            The catalogue is only read (close Lightroom first); sidecars
                                            already there are kept. --remap /Volumes/Photos/=/mnt/photos/
                                            finds photos that have moved
    amb denoise <RAW>... [--amount 0-100] [--out OUT.dng] [--json]
                                            AI Denoise: each raw without its noise, as a new DNG beside
                                            it (<name>-Denoised.dng) with the photo's edit; --amount
                                            mixes in the photo as it was (default 70) to keep texture;
                                            needs the denoise model (amb models install rawnind)
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
    amb docs [NAME...] [--to DIR]           The guides for AI agents, built in: the list, one guide
                                            (guide, core, features, settings, mcp, layouts, plugins, …),
                                            or all written to DIR with their links working
    amb man                                 Print this help as a manual page (man amb; docs/man/amb.1)
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
    --quality 1-100                         JPEG/AVIF quality (default 100)
    --bits 8|16|32                          PNG 8/16, TIFF 8/16/32 (float); default 8, TIFF 16
    --color-space srgb|p3|adobergb|prophoto|rec2020   Default srgb; an ICC profile is embedded
    --long-edge N | --short-edge N | --width N | --height N | --fit WxH | --megapixels M | --percent P
    --exact WxH                             Exactly W×H (e.g. 1080x1350): centre-cropped if the aspect differs
    --enlarge                              Allow upscaling (default: never enlarge)
    --sharpen screen|matte|glossy[:low|standard|high]   Output sharpening
    --metadata all|no-location|copyright|none           EXIF to include (default no-location)
    --max-size 500k|2M                      JPEG: lower the quality until the file fits
    --small-copy 3M                         Also write NAME-small.jpg, at most that size (as sharp as fits,
                                            smaller in pixels only when it must be), for sharing
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

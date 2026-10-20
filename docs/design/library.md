# The library

Status: built (2026-10-08). Asked for while preparing a 630-photo shoot: rate, label and tag many photos at once, and give a whole series the same edit.

![The library: the folder as a grid, four photos selected, their shared rating, label and keyword in the panel](../images/library.png)

## What it is

- **A folder as a grid.** The Library button (or G) swaps the editor for a grid of the open photo's folder, or any folder you choose. G again, Esc, Enter or a double-click goes back to the editor; Enter and double-click open that photo.
- **The files are the library.** Everything is read from and written to the photos' XMP sidecars, the files Lightroom and Bridge use. There is no database to import into or keep in sync.
- **The catalogue only remembers** (2026-10-15): what was read from each photo (its sidecar's rating, label, flag, keywords and metadata, whether it's edited, and the raw header's capture time, camera, lens and exposure) is kept in `catalog.sqlite` in the config folder, keyed by path and checked against the photo's and sidecar's size and modified time. A changed file, by us or another app, is read again; deleting the catalogue loses nothing. A folder of 3000 raws opens in about 0.1 s instead of 5 s once it has been shown (`amb catalog scan` on 4 Linux cores).
- **Thumbnails** are the camera's embedded previews first (quick). Photos with edits then get a render of their edit, one at a time, cached until the raw or its sidecar changes. The filmstrip does the same.

## What you can do

- **Find:** sort by name, capture time, rating or "edited first" (and reverse), filter by rating, colour label, flag and edited or not, and search names and keywords.
- **Select:** click, Shift-click for a range, Ctrl (⌘)-click for more; arrow keys move (Shift extends); Ctrl+A all shown, Ctrl+D none.
- **Rate, label and tag the selection:**
  - 1–5 rate and 0 clears; 6–9 set Red, Yellow, Green or Blue, and the same key again removes it (Lightroom's keys)
  - the panel shows what the selection shares: keywords on every photo (× removes from all) and on some of them (+ adds to the rest)
  - P flags a pick, X a reject, U removes the flag (Lightroom's keys); rejected photos are dimmed
  - written to each photo's `.xmp` at once: `xmp:Rating`, `xmp:Label`, `dc:subject`, and the flag (below)
- **Look closer** (Lightroom's culling views, buttons in the bar):
  - **Loupe** (E): one photo large; ← → step through the shown photos.
  - **Compare** (C): the select on the left, a candidate on the right (the first two selected, or the photo after the select). ← → change the candidate, ↓ swaps the two, ↑ makes the candidate the select and brings the next photo in.
  - **Survey** (N): the selected photos tiled as large as they fit (or the next twelve when one is selected). Click one to make it active; × takes it out of the survey and the selection.
  - In compare and survey, rating, label and flag keys and the panel act on the active photo, framed in the accent colour, as in Lightroom. Enter or a double-click opens it in the editor; G or Esc goes back to the grid.
  - Large previews are the camera's embedded preview at 1024 or 2048 pixels, then the edit's render for edited photos (both cached).
- **Virtual copies** (Ctrl+', or Make a virtual copy in the panel): a second edit of the same photo, listed right after it as "IMG_0001.nef · Copy 2" (below).
- **Copy and paste an edit** (Lightroom's Sync): Copy settings on one photo (Ctrl+Shift+C), select others, Paste settings (Ctrl+Shift+V).
- **Run a plugin…** on the selection: each photo gets the plugin's result for its own settings ([ai/plugins.md](../ai/plugins.md)).
  - you choose the setting groups; crop, transform, masks and retouching are off by default
  - a ticked group becomes exactly the source's: settings the source doesn't have go back to their defaults
  - it runs as protocol commands: the open photo gets one undo step, the others their sidecars
- **Export the selection** with the export dialog's choices (sizes, borders, sharpening, metadata), one file each into a folder, each with its own edit, rating, label and keywords.
- **Make a collage** with the selection in the collage maker's tray.
- **Merge to HDR** two or more bracketed raws into one floating-point DNG next to them ([hdr-merge.md](hdr-merge.md)).
- **Merge to Panorama** two or more overlapping raws into one DNG next to them ([panorama.md](panorama.md)).
- **Import** (Import… in the bar, or Ctrl+Shift+I anywhere): copy photos from a card or folder into the library (below).
- **Subfolders:** tick Subfolders in the bar to show the folder's subfolders too; their photos are named with their folder (`2026-07/IMG_1.nef`).
- **Several folders:** + in the bar shows another folder with the one shown; their photos are named with their folder (`trip/IMG_1.nef`). "N more ×" goes back to the first folder alone, and Folder… starts again. After an import into dated folders, all of them are shown.
- **All photos** (Show → All photos): every photo in the catalogue, from every folder shown so far, newest first. The search box finds names, keywords, titles, captions, cameras and lenses. `amb catalog find` searches it from the command line.
- **Smart collections** (Show → New smart collection…): photos chosen by rules, worked out each time from the catalogue, in the folders you name and their subfolders. Rules: rating, colour label, flag, keyword, file name, edited, capture date, camera, lens, ISO and focal length; all or any of them. Each is a small JSON file beside the collections (`amb_library::smart`). Edit rules… and Delete collection… in the panel; ↻ works them out again.
- **Live folders:** the library's folders and the filmstrip's are watched (`notify`); photos added, removed or renamed outside the app show up within a second, keeping the selection. Sidecar writes don't count, so our own saves don't reload anything.
- **Collections:** the menu next to Folder… shows a collection instead of the folder. The panel adds the selection to a collection (or a new one); B adds the selection, or the open photo in the editor, to the last collection used, like Lightroom's quick collection. Viewing a collection, the panel can remove photos from it (the files stay), rename it or delete it. **Collection set…** files it under a set (Lightroom's collection sets), and the menu groups collections by set; the set is a `set` name in the collection's file.

- **Title, caption, creator and copyright** (under the keywords, in the library panel and in the editor): Lightroom's Metadata panel fields, kept in the sidecar as `dc:title`, `dc:description`, `dc:creator` and `dc:rights` and embedded in exports (EXIF ImageDescription, Artist and Copyright, and XMP). A creator and copyright replace the camera's own; "Copyright only" exports keep them. In the library the panel shows what the selection shares, or "(mixed)".

The rating, label, flag and keyword keys work in the editor too, on the open photo. Its rating block has the label swatches and the flag buttons.

## Library folders

Folders, or whole drives, that the catalogue keeps indexed (photo manager track, step 1; started 2026-10-19). The list is the user's, in `library-folders.json` in the config folder (`amb_library::roots`); the catalogue stays a cache.

- `amb catalog add DIR` / `remove DIR` changes the list; a folder inside one already listed adds nothing.
- `amb catalog index` reads every library folder: new and changed photos are read, photos gone are forgotten, with progress. A folder that can't be reached (an unplugged drive) is left alone: its photos stay listed, offline.
- `amb catalog folders` lists each folder with its photo count, marked offline where it can't be reached: what the folder tree will show.
- **In the editor** (2026-10-20): a **Folders** panel left of the grid lists the library folders and their subfolders with photo counts (+ adds one, × removes it and its photos from the catalogue; the files stay). Clicking a folder shows it, with Subfolders as ticked. The library folders are indexed in the background a few seconds after the start (progress under the tree), and checked every 30 seconds for a drive that came back, which is then indexed again.
- **Offline:** a folder that can't be reached is greyed out and marked offline. Showing it lists its photos from the catalogue, with their cached thumbnails in grey; opening one in the editor says to plug the drive in. When the drive is back, the folder is shown from it again.
- New photos: a folder on screen is watched, so photos added show within a second (↻ reads it again by hand). Library folders not on screen catch up at the next start or when their drive comes back.
- Still to come: a grid that only draws what's on screen (very large folders), and keeping the thumbnail cache within a size.

## Import

Lightroom's Import "Copy", without a catalogue. The dialog (and `amb import SOURCE DEST`) shows what will happen before anything is copied.

- **From and into:** a card or folder (with its subfolders, such as `DCIM/100CANON`), and a destination folder, remembered for next time.
- **Folders:** by date (`2026/2026-10-10`), by month, one folder per day, straight into the destination, or your own template.
- **Names:** keep the camera's, date and number (`20261010_0001`), date and camera name, or your own template. Tokens: `{name}` `{yyyy}` `{yy}` `{mm}` `{dd}` `{hh}` `{min}` `{ss}` `{seq}` `{camera}`; a `/` in a folder template makes a subfolder.
- **Dates** come from EXIF; a file without one uses its modified time (UTC). `{seq}` counts in capture order from the first number.
- **Companions:** a raw's same-name JPEG and `.xmp` sidecar get its new name, so they stay a pair and keep their edits.
- **Keywords for all** go into each imported photo's sidecar.
- **Develop preset:** a look for every photo that doesn't bring its own edit (a sidecar with develop settings keeps it), applied as the editor applies presets (`--preset` in `amb import`).
- **Backup copy:** a second copy of the originals, under their names from the card, in `<folder>/Imported on YYYY-MM-DD/` (`--backup DIR`).
- **Nothing is overwritten.** A photo whose exact bytes are already in its destination folder, under any name, is skipped as imported, so the same card can be imported again for its new photos. Another file with the same name makes the new one `name-2`.
- **Safe copies:** each file is copied to a hidden temporary name, checked for size, then renamed, and keeps its modified time.
- After the import the library shows the first folder that got photos.

Code: `crates/amb-library/src/import.rs` (plan and run), `amb import` in `crates/amb-cli`, `ui/import.js`.

## Collections

Named lists of photos from any folders, like Lightroom's collections. With no catalogue, each is a small JSON file in the app's config folder (`collections/`):

```json
{ "format": "ambrotype-collection/1", "name": "Portfolio", "photos": ["/photos/2026/a.nef"] }
```

- Photos are kept by full path, in the order they were added; the grid can sort them like a folder.
- Edits, ratings and flags stay in each photo's own sidecar, so they are the same in the folder and every collection.
- A photo moved or deleted outside Ambrotype is reported when the collection opens, and stays listed until removed.

Code: `crates/amb-library/src/collections.rs`, `ui/collections.js`.

## Virtual copies

A virtual copy is a second edit of a photo: black and white beside colour, a square crop beside the full frame. Lightroom keeps them in its catalogue; ours are sidecars.

- `IMG_0001.nef`'s copies are `IMG_0001.copy-2.xmp`, `IMG_0001.copy-3.xmp`, … next to it (`photo.jpg.copy-2.xmp` for a JPEG). Lightroom reads only `IMG_0001.xmp`, so it doesn't see them.
- A copy starts with the photo's edit as it is (unsaved changes included) and its keywords, but no rating, label or flag, as in Lightroom. From then on it has its own edit, rating, label, flag and keywords.
- In the app a copy is its sidecar's path: the library, filmstrip, editor, collections and export treat it as a photo of its own. `amb_engine::virtual_copy_of` finds the photo it belongs to (a copy file without its photo is ignored), and `source_path` gives the file to decode.
- Delete copy (in the panel, when only copies are selected) removes the copy's sidecar, never the photo.
- Import brings copies along under the photo's new name (`20261010_0001.copy-2.xmp`).
- `amb render IMG_0001.copy-2.xmp out.jpg --preset IMG_0001.copy-2.xmp` renders a copy.

## Flags

Lightroom keeps pick and reject flags in its catalogue, not in sidecars, so there is no standard place for them. Ours:

- `amblib:Flag="pick"` or `"reject"`, in Ambrotype's library namespace (`http://ns.ambrotype.org/library/1.0/`). Lightroom ignores it and keeps it.
- Flags are separate from stars, as in Lightroom: a rejected photo keeps its stars.
- A rejected photo with no stars also gets `xmp:Rating="-1"`, the XMP way to say rejected, which Bridge and darktable read. A sidecar with `-1` from those apps opens as rejected.
- Rejected photos are not deleted or hidden; filter "Not rejected" leaves them out.

## History and snapshots

Lightroom keeps each photo's edit history and snapshots in its catalogue. Ours go in the sidecar, so they travel with the photo:

- `amblib:History`: JSON text with the undo and redo steps (`version` 1), each step's label and the settings it changed (before and after).
- It also carries a fingerprint of the settings the history ends at. If another app (or a hand edit) changes the sidecar's settings later, the history no longer fits and is dropped on opening, so undo never puts back values that no longer match. Changing the rating, label, flag or keywords elsewhere leaves it alone.
- At most about 512 KB is kept, newest steps first (brush strokes store the whole mask before and after). When older steps are left out, the History panel starts at "Earlier edits" instead of "Original".
- Anything that edits through `DevelopState` keeps it: the editor, library paste, `amb edit`, plugins and the MCP server. Exports never include it, and virtual copies start their own.
- `amblib:Snapshots`: JSON text with each snapshot's name and full settings (`version` 1). Snapshots don't depend on the current settings, so they are always kept, and they are never trimmed.
- The protocol commands are `save_snapshot` (also updates one), `apply_snapshot` (one undoable step), `rename_snapshot`, `delete_snapshot` and the `list_snapshots` query. Saving, renaming and deleting aren't edits and add no history step, as in Lightroom.
- Lightroom may drop both when it rewrites the sidecar; then the photo simply opens with no history or snapshots. To check in the trial month.

## Stacks

Lightroom keeps stacks in its catalogue. Ours go in the sidecar: `amblib:Stack` names the stack and `amblib:StackTop="true"` marks its top photo.

- The grid shows a closed stack as its top photo with a ⧉ count badge; clicking the badge opens or closes it.
- **Stack** (Ctrl+G) groups the selected photos, the first one on top. **Unstack** (Ctrl+Shift+G) takes the selected photos out.
- **Auto-stack…** groups photos taken within a number of seconds of each other, by capture time.
- Lightroom doesn't read our stacks, and stacks across folders only show when both folders are listed.

## Moving from Lightroom Classic

**From Lightroom…** in the library (or `amb lrcat import Catalog.lrcat`, with `--apply` to write) reads a Lightroom Classic catalogue and moves what it knows into files we and Lightroom both read.

- Each photo's sidecar gets the catalogue's develop settings (`Adobe_imageDevelopSettings`, newer than any sidecar Lightroom wrote), its rating, colour label, pick or reject flag, keywords and stack, and the metadata in the catalogue's XMP (title, caption, creator, copyright, …).
- Virtual copies become ours, `IMG.copy-2.xmp`, `.copy-3`, … in the catalogue's order, so running it again finds the same names.
- Collections become ours, in their sets (sets in sets are joined: "Travel / Italy"); two of one name get the set added to the later one's name. The Quick Collection comes when it has photos. Smart collections are named, not moved: they are rules, to make again here.
- Nothing is overwritten: a photo that already has a sidecar keeps it, and the catalogue is opened read-only and `immutable` (no lock, no journal next to it). Close Lightroom first, as changes it hasn't saved into the file aren't seen; a `.lock` or `-wal` next to the catalogue gives a warning.
- Photos that have moved (another disk or computer) are found with one path replacement: "Where Lightroom had them" / "Where they are now" (`--remap OLD=NEW`).
- The tables and columns read are the documented ones other free catalogue readers use; optional ones (keywords, stacks, collections) are skipped when a catalogue lacks them. Tested on synthetic catalogues only so far.

## Code

- `crates/amb-library/src/folder.rs`: which files in a folder are photos, in order; `catalog.rs`: the catalogue.
- `crates/amb-gui/src/library.rs`: listing, batch info changes, paste commands, batch export.
- `crates/amb-gui/ui/library.js`: the grid, selection, panel and keys.
- `crates/amb-gui/ui/cull.js`: loupe, compare and survey.
- `crates/amb-library/src/lrcat.rs` and `crates/amb-gui/ui/lrcat.js`: moving from Lightroom Classic.
- Labels and flags in `amb_protocol::sidecar::PhotoInfo` (`xmp:Label`, `amblib:Flag`); labels are exported as XMP (`amb_export::exif::Tags::label`), flags are not.

## Not yet

- The extra folders aren't remembered after closing the app. Collection sets are one level deep (no sets in sets). A smart collection doesn't update by itself when a photo's rating changes while it is shown (↻ does).
- The catalogue: All photos lists the folders shown before, the library folders (below) and anything `amb catalog scan`ned. Since 2026-10-19 it lists what was read last time without looking at each file, so it stays quick; a sidecar changed by another app shows when its folder is shown or indexed again. Photos on a drive that isn't plugged in stay listed, dimmed as offline; their previews come back with the drive. On file systems that keep modified times to the second (FAT and exFAT cards, old Mac disks), two changes to a sidecar of the same size within one second can be missed until the next change. Smart collections still look through their folders on disk rather than querying it.
- Collections don't follow photos moved outside Ambrotype.
- From Lightroom: smart collections, publish services, edit history and snapshots, face regions, keyword hierarchies (they come as plain keywords) and Lightroom's virtual copy names. One path replacement only.
- Import: moving instead of copying, ejecting the card, picking single photos to import. (A develop preset and a second backup copy came 2026-10-13.)
- Zooming in the loupe and compare views (open the photo in the editor for 100%).
- The rest of Lightroom's metadata panel (location, IPTC contact fields, headline) and editing keyword hierarchies. Camera details are shown under the histogram, not in the panel.
- Undo for library changes. Ratings, labels and keywords are written straight away; pasted edits can be undone per photo in the editor.
- Background work: big pastes and exports run while you wait, with progress in the status bar.

# The library

Status: built (2026-10-08). Asked for while preparing a 630-photo shoot: rate, label and tag many photos at once, and give a whole series the same edit.

![The library: the folder as a grid, four photos selected, their shared rating, label and keyword in the panel](../images/library.png)

## What it is

- **A folder as a grid.** The Library button (or G) swaps the editor for a grid of the open photo's folder, or any folder you choose. G again, Esc, Enter or a double-click goes back to the editor; Enter and double-click open that photo.
- **No catalogue.** Everything is read from and written to the photos' XMP sidecars, the files Lightroom and Bridge use. There is no database to import into, keep in sync or lose. The cost: one folder at a time, and a big folder is read when it opens (capture times come from the raw headers, read in parallel).
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
- **Copy and paste an edit** (Lightroom's Sync): Copy settings on one photo (Ctrl+Shift+C), select others, Paste settings (Ctrl+Shift+V).
  - you choose the setting groups; crop, transform, masks and retouching are off by default
  - a ticked group becomes exactly the source's: settings the source doesn't have go back to their defaults
  - it runs as protocol commands: the open photo gets one undo step, the others their sidecars
- **Export the selection** with the export dialog's choices (sizes, borders, sharpening, metadata), one file each into a folder, each with its own edit, rating, label and keywords.
- **Make a collage** with the selection in the collage maker's tray.
- **Import** (Import… in the bar, or Ctrl+Shift+I anywhere): copy photos from a card or folder into the library (below).

The rating, label, flag and keyword keys work in the editor too, on the open photo. Its rating block has the label swatches and the flag buttons.

## Import

Lightroom's Import "Copy", without a catalogue. The dialog (and `amb import SOURCE DEST`) shows what will happen before anything is copied.

- **From and into:** a card or folder (with its subfolders, such as `DCIM/100CANON`), and a destination folder, remembered for next time.
- **Folders:** by date (`2026/2026-10-10`), by month, one folder per day, straight into the destination, or your own template.
- **Names:** keep the camera's, date and number (`20261010_0001`), date and camera name, or your own template. Tokens: `{name}` `{yyyy}` `{yy}` `{mm}` `{dd}` `{hh}` `{min}` `{ss}` `{seq}` `{camera}`; a `/` in a folder template makes a subfolder.
- **Dates** come from EXIF; a file without one uses its modified time (UTC). `{seq}` counts in capture order from the first number.
- **Companions:** a raw's same-name JPEG and `.xmp` sidecar get its new name, so they stay a pair and keep their edits.
- **Keywords for all** go into each imported photo's sidecar.
- **Nothing is overwritten.** A photo whose exact bytes are already in its destination folder, under any name, is skipped as imported, so the same card can be imported again for its new photos. Another file with the same name makes the new one `name-2`.
- **Safe copies:** each file is copied to a hidden temporary name, checked for size, then renamed, and keeps its modified time.
- After the import the library shows the first folder that got photos.

Code: `crates/amb-library/src/import.rs` (plan and run), `amb import` in `crates/amb-cli`, `ui/import.js`.

## Flags

Lightroom keeps pick and reject flags in its catalogue, not in sidecars, so there is no standard place for them. Ours:

- `amblib:Flag="pick"` or `"reject"`, in Ambrotype's library namespace (`http://ns.ambrotype.org/library/1.0/`). Lightroom ignores it and keeps it.
- Flags are separate from stars, as in Lightroom: a rejected photo keeps its stars.
- A rejected photo with no stars also gets `xmp:Rating="-1"`, the XMP way to say rejected, which Bridge and darktable read. A sidecar with `-1` from those apps opens as rejected.
- Rejected photos are not deleted or hidden; filter "Not rejected" leaves them out.

## Code

- `crates/amb-gui/src/library.rs`: listing, batch info changes, paste commands, batch export.
- `crates/amb-gui/ui/library.js`: the grid, selection, panel and keys.
- `crates/amb-gui/ui/cull.js`: loupe, compare and survey.
- Labels and flags in `amb_protocol::sidecar::PhotoInfo` (`xmp:Label`, `amblib:Flag`); labels are exported as XMP (`amb_export::exif::Tags::label`), flags are not.

## Not yet

- Subfolders, several folders at once, collections, stacks. An import into dated folders shows only the first folder afterwards.
- Import: moving instead of copying, a second backup copy, ejecting the card, a develop preset on import, picking single photos to import.
- Zooming in the loupe and compare views (open the photo in the editor for 100%).
- A metadata panel (EXIF, caption, copyright) and editing keyword hierarchies.
- Undo for library changes. Ratings, labels and keywords are written straight away; pasted edits can be undone per photo in the editor.
- Background work: big pastes and exports run while you wait, with progress in the status bar.

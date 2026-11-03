# What Ambrotype can do

Find what you want to do (Ctrl+F works, or ask your AI to read this page). Each line says what you'd want, what does it, where to find it in the app, and the `amb` command when there is one. Names in **bold** are what you click.

In the app, **Search Everything** (Ctrl+K) finds this list too: type what you want ("whatsapp", "raw by mistake", "crooked horizon"), and it shows where it is, with a button for it.

Ctrl is ⌘ on a Mac. "Right-click" means on the photo, or on a thumbnail in the library.

## Getting started

- **I'm new to editing** → the empty window's **Try a sample photo** (a nine-step edit to step through in History) and **Watch an edit** (a cursor replays it). Help → **Learn** (F1) has short lessons, Help → **Photography basics** (Shift+F1) explains exposure, tones and colour on your own photo, and every setting has a **?** to hover or click.
- **Open a photo or a folder** → **Open a photo** (Ctrl+O), **Open a folder** (Ctrl+Shift+O), or drop it on the window. Raws (NEF, CR2/CR3, ARW, RAF, DNG, …), JPEG, PNG, TIFF and WebP.
- **Switch between library and editing** → **Library Grid** (G), or the task bar at the top: **Library**, **Develop**, **Color**, **Retouch**, **Film**, **Collage** (Ctrl+Alt+1–6).
- **Find a command or setting I can't see** → **Search Everything** (Ctrl+K): type its name, or a setting and a number ("exposure 0.7").
- **See all shortcuts, or change them** → **Keyboard Shortcuts** (Shift+?) shows them; **Settings** (Ctrl+,) changes them.

## Editing a photo

- **Brighter, darker, more contrast, recover highlights or shadows** → the **Basic** panel: **Exposure**, **Contrast**, **Highlights**, **Shadows**, **Whites**, **Blacks**. Auto (in Tone) does it for you.
- **Wrong colours, too blue or yellow** → white balance in **Basic**: As Shot, Auto, or the **White Balance Picker** (W) and a click on something grey or white.
- **More or less colourful** → **Vibrance** and **Saturation**; one colour at a time in the **Color Mixer**.
- **Drag on the photo instead of hunting for a slider** → **Touch** (T): drag up or down on a tone or a colour.
- **Black and white** → the **Black & White** panel; it sets how bright each colour becomes in grey.
- **A film look, a tint, sepia** → **Color Grading**, a LUT (in **Basic**), or a preset; for scanned negatives, the film stocks in **Film Negative**.
- **Fuji film recipe from the internet** → Presets → **Fuji recipe…** (paste it), or **Plugins** → Film recipe. `amb fuji RECIPE.txt PHOTO`.
- **Film grain, halation, bloom, highlights that fade like a print** → **Effects**: Grain (with Film response), Glow, Highlight roll-off.
- **Sharper, less noise** → **Detail**: Sharpening, Noise Reduction.
- **Crooked horizon** → **Crop and Straighten** (R), then **Level the horizon** (L), or drag outside the frame to turn it by hand.
- **Crop** → **Crop and Straighten** (R), then drag corner to corner like a screenshot; X swaps portrait and landscape, Enter applies.
- **Converging lines on buildings** → **Transform** → Upright.
- **Lens distortion, dark corners, colour fringes** → **Lens Corrections**: profile corrections (lensfun database or your Adobe `.lcp` files), Remove Chromatic Aberration, Defringe → Auto. Sony and DNG raws bring their own distortion correction. `amb lens info RAW`.
- **Change only part of the photo** → **Masks** (M): linear (L), radial (Shift+R), brush (B), luminance and colour ranges, Sky, and AI Subject, Background and People masks (they run on your computer; **Settings** → AI models).
- **Remove a spot, a blemish, a wire** → **Spot Removal** (Q): click it or brush over it; Heal, Clone or Remove (AI Remove fills it with what belongs there, once its model is installed in **Settings** → AI models).
- **Undo, or go back to an earlier version** → **Undo** (Ctrl+Z); the History panel (click any step); Snapshots (+ keeps the edit under a name).
- **See before and after** → **Before / after** (`\`); **Before / after split** (Y) has a bar to drag.
- **Check focus and detail** → **Fit / 100%** (Z or Space), **Loupe** (I), or right-drag a box to zoom to it.
- **Clipped whites or blacks** → **Clipping Overlay** (J).
- **Will it print or look right elsewhere** → **Soft Proofing** (S) shows the export's colour space.
- **Make the same edit on many photos** → **Copy Settings** (Ctrl+Shift+C) and **Paste Settings** on a selection (Ctrl+Shift+V), or **New Preset from This Photo** (Ctrl+Shift+N) and apply it.
- **Match the look of another photo** → Presets → **Match look…**. `amb match PHOTO REFERENCE`.
- **Surprise me with a look** → Presets → **Surprise me…**, or **Plugins** → Random look.
- **Let an AI edit the photo** → **Assistant** (Ctrl+Shift+A), or **Connect an AI App** (Help menu; `amb mcp`).
- **Edit in Photoshop or another app** → **Edit in Another App** (Ctrl+Alt+E; choose the app in **Settings** → General).

## Presets

- **Use my Lightroom presets** → Lightroom's preset folder is found by itself; **Add folder…** adds others. `.xmp` and `.lrtemplate` work, with an Amount slider.
- **Save my own preset** → **New Preset from This Photo** (Ctrl+Shift+N, or **+ New preset…** in Presets). It works in Lightroom too.
- **What's inside a preset, and what don't you support yet** → `amb preset inspect PRESET.xmp`.
- **Find presets, layouts and themes others made** → **Browse the Hub** (Ctrl+Shift+H).

## Film negatives and scans

- **Turn a scanned negative into a positive** → the **Film** task → **Film Negative**; **Pick base** and click the clear film between frames.
- **Make a whole roll look consistent** → in the library, **Sync negative roll…**, or Fixed range in **Film Negative**. `amb negative analyze`.
- **Colour cast from a sky or lamp, faded expired film** → **Film Negative**: Cast removal, Expired film; Auto Density and Auto Grade.
- **Dust and hairs on a scan** → **Spot Removal** → **Find dust**; Shift-drag along a straight scratch. `amb dust PHOTO --apply`.
- **Contact sheet of a roll** → in the library, **Contact sheet…** (film strips with frame numbers, or a plain grid). `amb contact`.

## Library and organising

- **Import from a memory card** → **Import Photos** (Ctrl+Shift+I): copies into dated folders, renames, skips duplicates. `amb import CARD DEST`.
- **Move from Lightroom Classic** → Import ▾ → from a Lightroom catalogue (edits, ratings, keywords, collections). `amb lrcat import CATALOG.lrcat`.
- **Rate, label, flag, keyword** → 1–5 rate, 6–9 colour labels, **Pick** (P), **Reject** (X), **Add Keywords** (K). `amb tag`.
- **Cull fast** → **Loupe** (E), **Compare** (C), **Survey** (N); auto-advance after a rating (**Settings** → General).
- **Find a photo by what's in it ("dog on a beach")** → the library's Smart search, or right-click → Find similar photos. `amb search "dog on a beach" DIR`.
- **Find photos by camera, lens, date, rating** → the library's filters and search. `amb catalog find`.
- **Duplicates and bursts, keep the best** → Organise ▾ → duplicates and bursts. `amb duplicates DIR`.
- **Sort into dated folders, move, rename** → Organise ▾ (sidecars go along). `amb sort`, `amb move`, `amb rename`.
- **Delete** → to the system trash, with all its files. `amb trash`.
- **Photos moved or renamed outside the app** → Organise ▾ → find moved photos. `amb catalog missing`, `amb catalog relink DIR`.
- **Groups of photos** → collections and smart collections; **Stack** (Ctrl+G); **Make a Virtual Copy** (Ctrl+').
- **Where was this taken** → the library's Map view; place photos from a GPX track or by dropping them on the map. `amb place DIR --gpx TRACK.gpx`.
- **Browse by date** → the library's Timeline view.
- **Whole drives, also unplugged** → Folders → add a drive. `amb catalog add DIR`.
- **Subfolders** → they're the first tiles in the grid; double-click a folder in the tree to open or close it.
- **Slideshow** → **Slideshow** (Ctrl+Enter in the library). `amb slideshow DIR`.

## Getting photos out

- **Export** → **Export** (Ctrl+E): JPEG, PNG, TIFF, WebP, AVIF, OpenEXR or JPEG XL; size, sharpening, metadata, colour space, border, watermark. `amb render PHOTO OUT.jpg`.
- **Many photos at once** → select them in the library (or the filmstrip) and **Export**. `amb render PHOTOS… --out-dir DIR`.
- **File too big to send (WhatsApp, email)** → **Export** → Keep under (MB); 16 MB is WhatsApp's maximum. `amb render … --max-size 16M`.
- **A big file and a small one for social media** → **Export** → Also a smaller copy, up to N MB (saves NAME-small.jpg beside it). `amb render … --small-copy 3M`.
- **Exactly 1080 × 1350 for Instagram** → **Export** → Size preset, or Exact size.
- **Shot RAW by mistake, want JPEGs like the camera makes** → **Convert raws** (library, File menu, or right-click): as the camera would have saved them, Auto, or like one photo you edited. `amb convert CARD --to JPEGS`.
- **Paste the photo into a chat or document** → right-click → Copy image.
- **A border, a Polaroid frame, a caption** → **Frame** (Shift+B).
- **A watermark or logo** → **Export** → Watermark.
- **Show before and after online** → **Share Before and After** (File menu; one web page with a slider). `amb share PHOTO --out page.html`.
- **A video of my edit for Reels or Shorts** → **Export Edit Replay** (File menu). `amb replay PHOTO --out edit.mp4`.
- **Collage** → **Collage Maker** (C): layouts, drag photos in, export pages. `amb collage`.
- **Merge brackets into HDR, or photos into a panorama** → the library's Merge to HDR… / Merge to Panorama…. `amb merge hdr`, `amb merge pano`.
- **Keep my export choices** → **Export** → Save as preset….

## Make it yours

- **A different layout or colours** → **Appearance** (Ctrl+L): built-in layouts and themes; **Edit Layout**, or **New Layout from Scratch**.
- **Hide everything but the photo** → the Minimalism layout in **Appearance**, or fold the **Left Column** (Ctrl+[), **Right Column** (Ctrl+]) and the top bar (Ctrl+Shift+T).
- **Small tools that edit for me** → **Plugins** (Ctrl+Shift+P); write your own ([ai/plugins.md](ai/plugins.md)).
- **Use my own AI model for masks** → **Settings** → AI models → add an ONNX model ([own-models.md](own-models.md)).
- **Camera profiles (.dcp), my ColorChecker profile** → **Basic** → Profile.

## Scripts and AI

- **Script edits** → `amb do PHOTO "exp=+0.5 con+=10"`, or `amb edit` with protocol commands ([ai/README.md](ai/README.md)).
- **Let my AI assistant edit and organise** → **Connect an AI App** (Help menu; `amb mcp`) ([ai/mcp.md](ai/mcp.md)).
- **Every setting, its range and meaning** → [ai/settings-reference.md](ai/settings-reference.md), `amb settings reference`.
- **Something's missing or not like Lightroom** → [GAPS.md](GAPS.md) lists what's approximate or not built yet; **Send Feedback** (Help menu).

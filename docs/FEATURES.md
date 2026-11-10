# What Ambrotype can do

Find what you want to do (Ctrl+F works, or ask your AI to read this page). Each line says what you'd want, what does it, where to find it in the app, and the `amb` command when there is one. Names in **bold** are what you click.

In the app, this list is Help → **What Ambrotype Can Do**, with a filter, and **Search Everything** (Ctrl+K) finds it too: type what you want ("whatsapp", "raw by mistake", "crooked horizon"), and it shows where it is, with a button for it.

Ctrl is ⌘ on a Mac. "Right-click" means on the photo, or on a thumbnail in the library.

## Getting started

- **I'm new to editing** → the empty window's **Try a sample photo** (a nine-step edit to step through in History) and **Watch an edit** (a cursor replays it). On your own photo: Help → **Watch an edit on this photo** (Auto edit's changes made one slider at a time, each a History step). Help → **Learn** (F1) has short lessons, Help → **Photography basics** (Shift+F1) explains exposure, tones and colour on your own photo, and every setting has a **?** to hover or click.
- **Learn what shutter speed, aperture, ISO and focal length do** → Help → **Camera simulator**: change them on a drawn scene and see the blur, depth of field, grain and zoom, with a line on what each did; **Auto exposure** sets the shutter.
- **Open a photo or a folder** → **Open a photo** (Ctrl+O), **Open a folder** (Ctrl+Shift+O), or drop it on the window. Raws (NEF, CR2/CR3, ARW, RAF, DNG, …), JPEG, PNG, TIFF and WebP.
- **Switch between library and editing** → **Library Grid** (G), or the task bar at the top: **Library**, **Develop**, **Film**, **Collage**, **Trippy** (Ctrl+Alt+1–5). Develop opens on **Quick fixes** and the **Retouch tools** (a click on a heading folds it), with its panels in four tabs on the right: Tone, **Colour** (grading wheels and mixer), Detail & lens, More. Collage opens the collage maker.
- **Find a command or setting I can't see** → **Search Everything** (Ctrl+K, or the magnifier in the toolbar): type its name, or a setting and a number ("exposure 0.7").
- **See everything the app can do** → Help → **What Ambrotype Can Do** (this list, with a filter).
- **Tips, or no tips** → a tip's **Don't show tips** (or **Not now** three times in a row) turns them off; **Settings** (Ctrl+,) → General: Tips while waiting, Tips in menus and empty boxes (both on) turns them on again.
- **See all shortcuts, or change them** → **Keyboard Shortcuts** (Shift+?) shows them; **Settings** (Ctrl+,) changes them.

## Editing a photo

- **Edit it for me, as a start** → **Auto** (the wand in the toolbar, Ctrl+U): tone, white balance, lens corrections, a little vibrance, and a crooked horizon levelled, in one undo step.
- **Brighter, darker, more contrast, recover highlights or shadows** → the **Basic** panel: **Exposure**, **Contrast**, **Highlights**, **Shadows**, **Whites**, **Blacks**. Auto (in Tone) does it for you.
- **Wrong colours, too blue or yellow** → white balance in **Basic**: As Shot, Auto, or the **White Balance Picker** (W) and a click on something grey or white.
- **More or less colourful** → **Vibrance** and **Saturation**; one colour at a time in the **Color Mixer**.
- **Drag on the photo instead of hunting for a slider** → **Touch** (T): drag up or down on a tone or a colour.
- **Black and white** → the **Black & White** panel; it sets how bright each colour becomes in grey.
- **A film look, a tint, sepia** → **Color Grading**, a LUT (in **Basic**), or a preset; for scanned negatives, the film stocks in **Film Negative**.
- **Fuji film recipe from the internet** → Presets → **Fuji recipe…** (paste it), or **Plugins** → Film recipe. `amb fuji RECIPE.txt PHOTO`.
- **Something wild: psychedelic colour, rainbow foil, a hologram look** → the **Trippy** task (Ctrl+Alt+5): **Trip me** (a new random mix each click), Acid, Pop art, Dream, Foil sticker, Oil slick, Prism, **Edges** and **Neon** (the edges glowing on black, as Visualise shows them), **Acid highlights**, **Foil shadows**; in **Effects** each effect's **Where** keeps it to the highlights, midtones, shadows or a colour (hover it to see where), and in Trippy its amount goes to 200 %; tune them in **Effects** → Plugin effects (Hue trip, Holographic). `amb plugin run plugins/hue-trip PHOTO --params turn=120`.
- **Film grain, halation, bloom, highlights that fade like a print** → **Effects**: Grain (with Film response), Glow, Highlight roll-off.
- **Sharper, less noise** → **Detail**: Sharpening, Noise Reduction.
- **Crooked horizon** → **Crop and Straighten** (R), then **Level the horizon** (L), or drag outside the frame to turn it by hand.
- **Crop** → **Crop and Straighten** (R), then drag corner to corner like a screenshot; X swaps portrait and landscape, Enter applies.
- **Converging lines on buildings** → **Transform** → Upright; or draw guides along the lines (**Guided**) and the sliders straighten them.
- **Lens distortion, dark corners, colour fringes** → **Lens Corrections**: profile corrections (lensfun's database, downloaded by itself at the first start when there is none, or your Adobe `.lcp` files), Remove Chromatic Aberration, Defringe → Auto. Sony and DNG raws bring their own distortion correction. `amb lens info RAW`.
- **Find the AI tools (remove, select subject, people, sky)** → Develop's **Retouch tools**, on the left under Quick fixes, a button each.
- **Change only part of the photo** → **Masks** (M): linear (L), radial (Shift+R), brush (B), luminance and colour ranges, Sky, and AI Subject, Background and People masks (they run on your computer; **Settings** → AI models).
- **A mask of something by naming it (a car, the dog, sky)** → **Masks** → **Describe it** (type and Enter), or **Select object**'s **Or type what**: sky, people, the subject or the background straight away; anything else is found by the search model and outlined by SAM 2.1 tiny (both in **Settings** → AI models; about 10–20 s).
- **Do something to one thing in the photo (brighten the person, move the boat, soften the background)** → **Select object** (**Select** in the toolbar, O, or first in **Retouch tools**): click it (Shift-click adds, Alt-click takes away, or drag a box), then, on the bar right under it, **Adjust** (a mask with its sliders), **Brighten**, **Darken**, **Soften background**, **Remove** or **Move** (drag it). A shadow it casts shows in blue: **Its shadow too** takes it along. Clicks need SAM 2.1 tiny in **Settings** → AI models.
- **Remove a person, a bin, a car, anything** → **Remove object…** (Shift+Q, first in the **Spot Removal** panel, or **Retouch tools**): click it (Alt-click for just a part) or drag a box round it; its outline is found (a click needs SAM 2.1 tiny in **Settings** → AI models) and AI Remove fills all of it in one spot.
- **Remove a spot, a blemish, a wire** → **Spot Removal** (Q): click it or brush over it; Heal, Clone or Remove (AI Remove fills it with what belongs there, once its model is installed in **Settings** → AI models: Big-LaMa for the best fills, MI-GAN for five times the speed).
- **Undo, or go back to an earlier version** → **Undo** (Ctrl+Z); the History panel (click any step); Snapshots (+ keeps the edit under a name).
- **See before and after** → **Before / after** (`\`): the photo as it was, with a bar at the side to drag in and see the edit; **Before / after split** (Y) puts the bar where you left it.
- **Check focus and detail** → **Fit / 100%** (Z or Space, or Ctrl+scroll at the pointer), **Loupe** (I), or right-drag a box to zoom to it. Zoomed in, the arrow keys or a right-drag move the photo; all of these also work with Spot Removal or Masks open.
- **Clipped whites or blacks** → **Clipping Overlay** (J).
- **Will it print or look right elsewhere** → **Soft Proofing** (S) shows the export's colour space.
- **Make the same edit on many photos** → **Copy Settings** (Ctrl+Shift+C) and **Paste Settings** on a selection (Ctrl+Shift+V), or **New Preset from This Photo** (Ctrl+Shift+N) and apply it.
- **Match the look of another photo** → Presets → **Match look…**. `amb match PHOTO REFERENCE`.
- **A wild look: toy camera, miniature (tilt-shift), infrared, cartoon, oil painting** → Presets → **Wild** (Toy camera, Miniature, Infrared, Cross-process, Dreamland, Acid, Hologram, Oil painting, Cartoon, Poster, Pop art); tune them in **Effects** → Plugin effects (Paint, Tilt-shift).
- **Surprise me with a look** → Presets → **Surprise me…**, or **Plugins** → Random look.
- **Let an AI edit the photo** → **Assistant** (Ctrl+Shift+A), or **Connect an AI App** (Help menu; `amb mcp`).
- **Edit the open photo from another app or an AI at the same time** → the editor shows the change at once, as one undo step.
- **Colours on my screen look off** → on Linux the preview is shown in the monitor's own profile (**Settings** → General: Automatic, Off or a file); macOS and Windows do it themselves.
- **Blown sky edge, flower or sunlit wall** → highlights where one channel still held detail get their colour and texture back by themselves; **Clipping Overlay** (J) shows what's left.
- **Edit in Photoshop or another app** → **Edit in Another App** (Ctrl+Alt+E; choose the app in **Settings** → General).

## Presets

- **Use my Lightroom presets** → Lightroom's preset folder is found by itself; **Add folder…** adds others. `.xmp` and `.lrtemplate` work, with an Amount slider.
- **Save my own preset** → **New Preset from This Photo** (Ctrl+Shift+N, or **+ New preset…** in Presets). It works in Lightroom too.
- **What's inside a preset, and what don't you support yet** → `amb preset inspect PRESET.xmp`.
- **Find presets, layouts and themes others made** → **Browse the Hub** (Ctrl+Shift+H).
- **Ready-made looks to start from** → Presets → **Preset library…**: 21 free presets by style (film, B&W, moody, bright, vivid, portrait, landscape); hover to preview, **Apply**, or **Add to your presets**.
- **A preset that replaces my edit, or adds to it** → Presets → **Apply to the original, not on top**: on, a preset replaces the look (crop, spots, masks and geometry stay); off, presets add up as in Lightroom.

## Film negatives and scans

- **Turn a scanned negative into a positive** → the **Film** task → **Film Negative**; **Pick base** and click the clear film between frames.
- **Make a whole roll look consistent** → in the library, **Sync negative roll…**, or Fixed range in **Film Negative**. `amb negative analyze`.
- **Colour cast from a sky or lamp, faded expired film** → **Film Negative**: Cast removal, Expired film; Auto Density and Auto Grade.
- **Dust and hairs on a scan** → **Spot Removal** → **Find dust**; Shift-drag along a straight scratch. `amb dust PHOTO --apply`.
- **Contact sheet of a roll** → in the library, **Contact sheet…** (film strips with frame numbers, or a plain grid). `amb contact`.

## Library and organising

- **Import from a memory card** → **Import Photos** (Ctrl+Shift+I): copies into dated folders, renames, skips duplicates. `amb import CARD DEST`.
- **Look through photos full screen, like an image viewer** → **V**, or **⛶ View** in the library bar or the filmstrip's bar: from the photo you're on, ← → (or Space) or a click on the left or right of the screen steps, Esc ends on the photo you got to. With **JPEG if there is one**, the camera's JPEGs. While viewing, 0–5 rate, P X U flag, 6–9 label, and **I** or a right-click shows the photo's info (camera, lens, exposure, date, size) with its rating, label, flag and keywords to change.
- **Go through the camera's JPEGs big, after a shoot** → **JPEG if there is one** (the library bar, or the filmstrip's bar in Develop): a raw shot with a JPEG shows as its JPEG, other raws stay, and ← → step through them. For the photo alone, big: the library's Loupe (E, ← → to step), or the buttons at the photo's top corners fold the side panels away.
- **See only the raws, or only the JPEGs, when browsing** → library → **Filter ▾** → **File**: Raw only, or JPEG only (a raw shot with a JPEG shows its JPEG then; normally they're one photo, shown as the raw).
- **Move from Lightroom Classic** → Import ▾ → from a Lightroom catalogue (edits, ratings, keywords, collections). `amb lrcat import CATALOG.lrcat`.
- **Use my Lightroom catalogue as it is, without importing** → library → Folders panel → **Lightroom +** (or From Lightroom Classic… → **Open in place**): its folders and collections show there, its photos with their Lightroom edits, ratings, labels and keywords; nothing is written to the catalogue, and an edit here goes into the photo's sidecar. `amb lrcat open CATALOG.lrcat`.
- **Turn a photo that's sideways, or mirror it** → **Rotate left** (Ctrl+[), **Rotate right** (Ctrl+]) or **Flip horizontal**, in the Photo menu: the open photo, or the photos selected in the library; in the full-screen viewer (V), the same keys or the ⟲ ⟳ buttons on its bar. Crops and masks turn with it, and Lightroom sees the turn too. `amb tag PHOTO --rotate left|right|180 --flip horizontal`.
- **Rate, label, flag, keyword** → 1–5 rate, 6–9 colour labels, **Pick** (P), **Reject** (X), **Add Keywords** (K). `amb tag`.
- **Pick the best shot of a burst (in focus, eyes open)** → library → Organise → **Best in each burst**: the best of each burst is marked with why, the rest dimmed with what's wrong (eyes closed, soft); then **Pick best, reject rest** or **Stack each**. A click on a dimmed photo's badge keeps it instead. Eyes need YuNet in **Settings** → AI models; without it, focus alone.
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
- **Name batch files by a template** → **Export** → file name ({name}, {seq}, the date, {camera}). `amb render PHOTOS… --out-dir DIR --name TEMPLATE`.
- **JPEG chroma subsampling, progressive JPEG** → **Export**; AVIF has a speed.
- **File too big to send (WhatsApp, email)** → **Export** → Keep under (MB); 16 MB is WhatsApp's maximum. `amb render … --max-size 16M`.
- **A big file and a small one for social media** → **Export** → Also a smaller copy, up to N MB (saves NAME-small.jpg beside it). `amb render … --small-copy 3M`.
- **Exactly 1080 × 1350 for Instagram** → **Export** → Size preset, or Exact size (the crop's size presets cover Instagram, TikTok, YouTube, print and more). `amb render --exact 1080x1350`.
- **Shot RAW by mistake, want JPEGs like the camera makes** → **Convert raws** (library, File menu, or right-click): as the camera would have saved them, Auto, or like one photo you edited (pick which from the edited ones). `amb convert CARD --to JPEGS`.
- **Paste the photo into a chat or document** → right-click → Copy image.
- **A border, a Polaroid frame, a caption** → **Frame** (Shift+B).
- **A watermark or logo** → **Export** → Watermark.
- **Show before and after online** → **Share Before and After** (File menu; one web page with a slider). `amb share PHOTO --out page.html`.
- **A video of my edit for Reels or Shorts** → **Export Edit Replay** (File menu). `amb replay PHOTO --out edit.mp4`.
- **Collage** → the **Collage** tab, or **Collage Maker** (C): layouts, drag photos in, export pages. `amb collage`.
- **Merge brackets into HDR, or photos into a panorama** → the library's Merge to HDR… / Merge to Panorama…. `amb merge hdr`, `amb merge pano`.
- **Keep my export choices** → **Export** → Save as preset….

## Make it yours

- **A different layout or colours** → **Appearance** (Ctrl+L): built-in layouts and themes; **Edit Layout**, or **New Layout from Scratch**.
- **Wider or narrower side columns** → drag a column's inner edge (double-click for the layout's width); it's kept per layout.
- **Keep a layout and theme together** → **Appearance** → **Save this setup…**; the one before your last change is kept by itself.
- **Words on every button, only the everyday tools** → the Beginner layout in **Appearance**.
- **Use the menu bar** → on a Mac it's at the top of the screen; on Windows and Linux, **Settings** → General shows it in the window. Every command is there with its keys.
- **Replace the whole window with my own interface** → **Settings** → General → Interface (a folder with an `index.html`, written by hand or by an AI; [ai/custom-interface.md](ai/custom-interface.md)); the menu bar's Interface → Back to the Built-in Interface, or `--builtin-ui`, always comes back.
- **Hide everything but the photo** → the Minimalism layout in **Appearance**, or fold the **Left Column** (F7), **Right Column** (F8) and the top bar (Ctrl+Shift+T).
- **Small tools that edit for me** → **Plugins** (Ctrl+Shift+P); write your own ([ai/plugins.md](ai/plugins.md)).
- **My own tab with my tools** → a task plugin: a layout and buttons in a `plugin.json`, no code; it joins the task bar ([ai/plugins.md](ai/plugins.md#task-plugins)). Trippy is one.
- **Make AI tools faster (Select object, AI Remove, Describe it)** → they run on the GPU by themselves when the computer has one; **Settings** → AI models says where they run (`amb models` too; `AMB_AI_GPU=off` keeps them on the CPU).
- **Use my own AI model for masks** → **Settings** → AI models → add an ONNX model ([own-models.md](own-models.md)).
- **Camera profiles (.dcp), my ColorChecker profile** → **Basic** → Profile.

## Scripts and AI

- **Script edits** → `amb do PHOTO "exp=+0.5 con+=10"`, or `amb edit` with protocol commands ([ai/README.md](ai/README.md)).
- **Let my AI assistant edit and organise** → **Connect an AI App** (Help menu; `amb mcp`) ([ai/mcp.md](ai/mcp.md)).
- **Every setting, its range and meaning** → [ai/settings-reference.md](ai/settings-reference.md), `amb settings reference`.
- **Something's missing or not like Lightroom** → [GAPS.md](GAPS.md) lists what's approximate or not built yet; **Send Feedback** (Help menu).

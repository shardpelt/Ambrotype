// SPDX-License-Identifier: GPL-3.0-or-later
// Keyboard shortcuts: one table of actions with their default keys, the
// user's changes (kept in the app's settings by src/keys.rs; only changes are
// stored, so new defaults still arrive), the key dispatcher, and the Settings
// dialog that lists them on a keyboard map and lets you change them.
//
// A key is written like "Ctrl+Shift+Z": Ctrl means Ctrl, or Cmd on a Mac.
"use strict";

const IS_MAC = /Mac|iPhone|iPad/.test(navigator.platform);

// `when`: "any", "main" (crop tool closed), "crop" (crop tool open),
// "masks", "spots", "library" (the library grid shown), or a list of them.
// Actions with overlapping contexts can't share a key. `photo`: needs an open photo.
// `button`: the toolbar button whose tooltip shows the keys.
// Colour labels, written to the sidecar as Lightroom does (`xmp:Label`).
const LABELS = [tKey("Red"), tKey("Yellow"), tKey("Green"), tKey("Blue"), tKey("Purple")];
// Muted and even in brightness, so no label shouts; readable on dark and light themes.
const LABEL_COLOURS = { Red: "#cf6a5e", Yellow: "#d2b260", Green: "#73a17c", Blue: "#6189c2", Purple: "#9580c0" };

// Flag keys work everywhere but the crop tool, where X swaps the aspect.
const FLAG_CONTEXTS = ["main", "masks", "spots", "library"];

const KEY_ACTIONS = [
  { id: "open", group: tKey("File"), label: "Open a photo", keys: ["Ctrl+O"], when: "any", button: "btn-open", run: () => openDialog() },
  { id: "open-folder", group: tKey("File"), label: "Open a folder", hint: "Its photos in the library.", keys: ["Ctrl+Shift+O"], when: "any", run: () => openFolder() },
  { id: "save", group: tKey("File"), label: "Save the edit to the sidecar", keys: ["Ctrl+S"], when: "any", button: "btn-save", run: () => save() },
  { id: "export", group: tKey("File"), label: "Export", keys: ["Ctrl+E"], when: "any", button: "btn-export", run: () => exportFlow() },
  { id: "replay", group: tKey("File"), label: "Export Edit Replay: a video of the edit, step by step", hint: "An MP4 (AV1) of the photo's History: before, each step easing in and named, after. For Reels, Shorts and before/after posts.", keys: [], when: "any", photo: true, run: () => replayFlow() },
  { id: "share-compare", group: tKey("File"), label: "Share before and after", hint: "One web page with the photo before and after its edit and a bar to drag; it works offline and on any web host.", keys: [], when: "any", photo: true, run: () => shareCompareFlow() },
  { id: "edit-in", group: tKey("File"), label: "Edit in another app", hint: "The photo, as edited, goes to your image editor as a TIFF next to it (set the app in Settings → General).", keys: ["Ctrl+Alt+E"], when: "any", photo: true, run: () => editInFlow() },
  { id: "collage-shown", group: tKey("File"), label: "Collage of every photo shown in the library, as pages", hint: "One layout over all of them: the collage's Batch exports the pages.", keys: [], when: "any", run: () => libraryCollage(true) },
    { id: "collage", group: tKey("File"), label: "Collage maker", hint: "Several photos on one canvas.", keys: ["C"], when: "main", button: "btn-collage", run: () => collageFlow() },
  { id: "undo", group: tKey("Edit"), label: "Undo", keys: ["Ctrl+Z"], when: "any", button: "btn-undo", run: () => run([{ cmd: "undo" }]) },
  { id: "undo-all", group: tKey("Edit"), label: "Undo all: back to the photo as it opened", hint: "Every step stays in History, so Redo brings them back.", keys: ["Ctrl+Alt+Z"], when: "any", button: "btn-undo-all", run: () => undoAll() },
  { id: "redo", group: tKey("Edit"), label: "Redo", keys: ["Ctrl+Shift+Z", "Ctrl+Y"], when: "any", button: "btn-redo", run: () => run([{ cmd: "redo" }]) },
  { id: "nav-back", group: tKey("Tasks"), label: "Back to the task before (Library, Develop, …)", keys: ["Alt+ArrowLeft"], when: "any", button: "btn-nav-back", run: () => navBack() },
  { id: "nav-forward", group: tKey("Tasks"), label: "Forward again, after Back", keys: ["Alt+ArrowRight"], when: "any", run: () => navGo(1) },
  ...[["library", tKey("Library task")], ["develop", tKey("Develop task")], ["film", tKey("Film task")], ["collage", tKey("Collage task")], ["trippy", tKey("Trippy task")]].map(([task, label], i) => ({ id: `task-${task}`, group: tKey("Tasks"), label, keys: [`Ctrl+Alt+${i + 1}`], when: "any", run: () => switchTask(task) })),
  // Installed task plugins after Trippy.
  ...[5, 6, 7, 8].map((n) => ({ id: `task-${n + 1}`, group: tKey("Tasks"), label: tKey("Task {n} (an installed task plugin)"), vars: { n: n + 1 }, keys: [`Ctrl+Alt+${n + 1}`], when: "any", run: () => TASKS[n] && switchTask(TASKS[n].id) })),
  { id: "trip-me", group: tKey("Effects"), label: "Trip me: a random mix of psychedelic colour, foil and light leaks", keys: [], when: "main", photo: true, run: () => tripMe() },
  { id: "trip-save", group: tKey("Effects"), label: "Trip: save as a preset, effects included, and share it to the hub", keys: [], when: "main", photo: true, run: () => tripSave() },
  { id: "trip-clear", group: tKey("Effects"), label: "Trip: back to normal (the trip's effects off)", keys: [], when: "main", photo: true, run: () => tripClear() },
  { id: "auto-edit", group: tKey("Edit"), label: "Auto edit: tone, white balance, lens corrections, vibrance and a level horizon", hint: "A starting point for the whole photo, in one undo step.", keys: ["Ctrl+U"], when: "main", button: "btn-auto", run: () => autoEditFlow() },
  { id: "search", group: tKey("Help"), label: "Search everything: commands, settings, panels, presets, layouts", keys: ["Ctrl+K"], when: "any", button: "btn-search", run: () => searchFlow() },
  { id: "connect-ai", group: tKey("Help"), label: "Connect an AI app", hint: "Any AI app that runs MCP servers, or a command-line agent started in a terminal, told where the guides are.", keys: [], when: "any", run: () => connectFlow().catch(showError) },
  { id: "ai-guides", group: tKey("Help"), label: "Guides for AI agents", hint: "Built in: how an AI drives Ambrotype, every setting, layouts, plugins. amb docs in a terminal.", keys: [], when: "any", run: () => readGuide("guide") },
  { id: "sample", group: tKey("Help"), label: "Open the sample photo", hint: "A raw with an edit already made: step through its History to see what each slider does.", keys: [], when: "any", run: () => sampleFlow() },
  { id: "watch", group: tKey("Help"), label: "Watch an edit", hint: "A ghost cursor edits the sample photo in about 20 seconds; any key or click stops it. Starts the sample's edit afresh.", keys: [], when: "any", run: () => watchFlow() },
  { id: "watch-own", group: tKey("Help"), label: "Watch an edit on this photo", hint: "A ghost cursor makes Auto edit's changes to your photo one slider at a time; each is a History step, and any key or click stops it.", keys: [], when: "any", photo: true, run: () => watchOwnFlow() },
  { id: "basics", group: tKey("Help"), label: "Photography basics", hint: "How a photo is made and edited, step by step, and what the terms mean.", keys: ["Shift+F1"], when: "any", run: () => basicsFlow() },
  { id: "camsim", group: tKey("Help"), label: "Camera simulator", hint: "Try shutter speed, aperture, ISO and focal length on a drawn scene and see what each does: blur, depth of field, grain, zoom.", keys: [], when: "any", run: () => camsimFlow() },
  { id: "learn", group: tKey("Help"), label: "Learn: short guided lessons", keys: ["F1"], when: "any", run: () => learnFlow() },
  { id: "features", group: tKey("Help"), label: "What Ambrotype can do: every feature, by what you want to do", keys: [], when: "any", run: () => featuresFlow() },
  { id: "before", group: tKey("View"), label: "Before / after: the photo as it was, with a bar to drag in", keys: ["\\"], when: "any", button: "btn-before", run: () => toggleBefore() },
  { id: "touch", group: tKey("Tools"), label: "Touch: drag on the photo to change the tone or colour under the pointer", hint: "Up or down changes the tone curve's region, or the colour's hue, saturation or brightness.", keys: ["T"], when: "main", photo: true, run: () => toggleTouch() },
  { id: "split", group: tKey("View"), label: "Before / after split", hint: "Before on the left of a bar, after on the right; drag the bar.", keys: ["Y"], when: "main", photo: true, run: () => toggleSplit() },
  { id: "side-by-side", group: tKey("View"), label: "Side by side", hint: "Before on the left, after on the right, each whole; zooming and panning move both. Or right-click Before / after.", keys: [], when: "any", photo: true, run: () => sbsSet(sbs.layout === "side" ? null : "side") },
  { id: "side-by-side-fill", group: tKey("View"), label: "Side by side, filling the view", hint: "Each half filled by the photo, the same part in both; drag to move both.", keys: [], when: "any", photo: true, run: () => sbsSet(sbs.layout === "fill" ? null : "fill") },
  { id: "above-below", group: tKey("View"), label: "Above and below", hint: "Before above, after below, each whole; zooming and panning move both.", keys: [], when: "any", photo: true, run: () => sbsSet(sbs.layout === "stacked" ? null : "stacked") },
  { id: "zoom", group: tKey("View"), label: "Fit / 100% view", hint: "Or click the photo; drag to pan at 100%. Right-drag a box to zoom to it.", keys: ["Z", "Space"], when: ["main", "masks", "spots", "select"], photo: true, button: "btn-zoom", run: (e) => (e.key === " " && e.target.matches("button, input, select, textarea, summary") ? false : setZoom(!ui.zoom.on, ui.zoom.cx, ui.zoom.cy)) },
  { id: "loupe", group: tKey("View"), label: "Loupe: see the part under the pointer at 100%", hint: "Scroll to magnify (100–400%). I or Esc closes it.", keys: ["I"], when: "main", photo: true, run: () => toggleLoupe() },
  { id: "proof", group: tKey("View"), label: "Soft proofing", hint: "See the photo as it will export to sRGB, Adobe RGB, Display P3 or ProPhoto, and mark the colours that won't fit.", keys: ["S"], when: "main", photo: true, run: () => toggleProof() },
  { id: "frame", group: tKey("View"), label: "Frame: border and text around the photo", keys: ["Shift+B"], when: "main", photo: true, button: "btn-frame", run: () => toggleFrame() },
  { id: "shortcuts", group: tKey("Help"), label: "Show all keyboard shortcuts", keys: ["Shift+?"], when: "any", button: "btn-keys", run: () => showShortcuts() },
  { id: "clipping", group: tKey("View"), label: "Clipping overlay", hint: "Red: clipped highlights, blue: crushed shadows.", keys: ["J"], when: "any", button: "btn-clip", run: () => toggleClipping() },
  { id: "crop", group: tKey("Tools"), label: "Crop and straighten", keys: ["R"], when: "any", photo: true, button: "btn-crop", run: () => toggleCrop() },
  { id: "masks", group: tKey("Tools"), label: "Masks", hint: "Gradients, brush and range masks with their own sliders.", keys: ["M"], when: "any", photo: true, button: "btn-masks", run: () => toggleMasks() },
  { id: "remove-object", group: tKey("Tools"), label: "Remove object", hint: "Draw a box round it: the Subject model finds its outline, and AI Remove fills it in one spot.", keys: ["Shift+Q"], when: ["main", "spots"], photo: true, run: async () => { await enterSpots(); if (sp.on) setObjectMode(true); } },
  { id: "select", group: tKey("Tools"), label: "Select object", hint: "Click a person, a car, the sky: then adjust it with a mask, remove it, move it, or soften the background.", keys: ["O"], when: ["main", "spots", "select"], photo: true, button: "btn-select", run: () => toggleSelect() },
  { id: "select-leave", group: tKey("Select object"), label: "Leave Select object", keys: ["Escape"], when: "select", run: () => (sl.moving ? (sl.moving = false, sl.offset = [0, 0], renderSelectPanel()) : exitSelect()) },
  { id: "spots", group: tKey("Tools"), label: "Spot removal", hint: "Heal or clone blemishes and distractions.", keys: ["Q"], when: "any", photo: true, button: "btn-spots", run: () => toggleSpots() },
  { id: "wb", group: tKey("Tools"), label: "White-balance picker", hint: "Then click something neutral grey or white.", keys: ["W"], when: "main", button: "btn-wb", run: () => setPicking(ui.picking === "white" ? false : "white") },
  { id: "pick-base", group: tKey("Tools"), label: "Film negative: pick the film base", hint: "Then click the clear film between frames. In the Film Negative panel, Alt-click Pick base to measure it from the scan again.", keys: [], when: "main", photo: true, run: () => pickBaseFlow() },
  { id: "meter-area", group: tKey("Tools"), label: "Film negative: meter area, the part Auto measures", hint: "Then drag a box round it, to leave out a sky, a light leak or a holder's edge. In the Film Negative panel, Alt-click Meter area to meter the whole crop again.", keys: [], when: "main", photo: true, run: () => meterAreaFlow() },
  { id: "leave", group: tKey("Tools"), label: "Leave the picker, Touch or side by side", keys: ["Escape"], when: "main", run: () => (sbs.layout ? sbsSet(null) : touchState.on ? toggleTouch(false) : setPicking(false)) },
  { id: "crop-draw", group: tKey("Crop tool"), label: "Draw a new crop (corner to corner)", keys: ["D"], when: "crop", run: () => ui.crop.tool.setDrawMode(!ui.crop.tool.drawMode()) },
  { id: "crop-level", group: tKey("Crop tool"), label: "Level the horizon", hint: "Straighten by the photo's long lines, and crop to fit.", keys: ["L"], when: "crop", button: "crop-level", run: () => levelCrop() },
  { id: "crop-swap", group: tKey("Crop tool"), label: "Swap portrait and landscape", keys: ["X"], when: "crop", run: () => swapCrop() },
  { id: "crop-apply", group: tKey("Crop tool"), label: "Apply the crop", keys: ["Enter"], when: "crop", run: (e) => (e.target.matches("button, select") ? false : exitCrop(true)) },
  { id: "crop-cancel", group: tKey("Crop tool"), label: "Cancel the crop", keys: ["Escape"], when: "crop", run: () => exitCrop(false) },
  { id: "mask-linear", group: tKey("Masks"), label: "New linear gradient", keys: ["L"], when: "masks", run: () => startTool("linear", true) },
  { id: "mask-radial", group: tKey("Masks"), label: "New radial gradient", keys: ["Shift+R"], when: "masks", run: () => startTool("radial", true) },
  { id: "mask-brush", group: tKey("Masks"), label: "New brush mask", hint: "Hold Alt to erase.", keys: ["B"], when: "masks", run: () => startTool("brush", true) },
  { id: "mask-overlay", group: tKey("Masks"), label: "Show or hide the mask overlay", keys: ["O"], when: "masks", run: () => toggleMaskOverlay() },
  { id: "mask-smaller", group: tKey("Masks"), label: "Smaller brush", keys: ["["], when: "masks", run: () => brushSize(-1) },
  { id: "mask-bigger", group: tKey("Masks"), label: "Bigger brush", keys: ["]"], when: "masks", run: () => brushSize(1) },
  { id: "mask-delete", group: tKey("Masks"), label: "Delete the selected part (or mask)", keys: ["Delete", "Backspace"], when: "masks", run: (e) => (onControl(e) ? false : deleteMaskPart()) },
  { id: "spot-smaller", group: tKey("Spot removal"), label: "Smaller spot", keys: ["["], when: "spots", run: () => spotSize(-1) },
  { id: "spot-bigger", group: tKey("Spot removal"), label: "Bigger spot", keys: ["]"], when: "spots", run: () => spotSize(1) },
  { id: "spot-delete", group: tKey("Spot removal"), label: "Delete the selected spot", keys: ["Delete", "Backspace"], when: "spots", run: (e) => (onControl(e) ? false : deleteSpot()) },
  { id: "spot-visualise", group: tKey("Spot removal"), label: "Visualise spots", keys: ["A"], when: "spots", run: () => setVisualise(!sp.visualise) },
  { id: "spot-leave", group: tKey("Spot removal"), label: "Leave spot removal", keys: ["Escape"], when: "spots", run: () => (sp.object ? setObjectMode(false) : exitSpots()) },
  { id: "mask-leave", group: tKey("Masks"), label: "Stop drawing, then leave the masks", keys: ["Escape"], when: "masks", run: () => maskEscape() },
  // Rating, labels and keywords: the open photo, or in the library the selected photos.
  ...[1, 2, 3, 4, 5].map((n) => ({ id: `rate-${n}`, group: tKey("Rating"), label: tKey("Rate {stars}"), vars: { stars: "★".repeat(n) }, keys: [String(n)], when: "any", run: () => rateKey(n) })),
  { id: "rate-0", group: tKey("Rating"), label: "Clear the rating", keys: ["0"], when: "any", run: () => rateKey(0) },
  ...LABELS.slice(0, 4).map((l, i) => ({ id: `label-${l.toLowerCase()}`, group: tKey("Rating"), label: tKey("{colour} label (again to remove)"), vars: { colour: t(l) }, keys: [String(6 + i)], when: "any", run: () => labelKey(l) })),
  { id: "rotate-left", group: tKey("Photo"), label: "Rotate left", hint: "Turn the photo a quarter turn anticlockwise (in the library, the selected photos). Crops, masks and spots turn with it.", keys: ["Ctrl+["], when: "any", run: () => turnPhotos({ rotate: -1 }) },
  { id: "rotate-right", group: tKey("Photo"), label: "Rotate right", hint: "Turn the photo a quarter turn clockwise (in the library, the selected photos). Crops, masks and spots turn with it.", keys: ["Ctrl+]"], when: "any", run: () => turnPhotos({ rotate: 1 }) },
  { id: "flip-horizontal", group: tKey("Photo"), label: "Flip horizontal", hint: "Mirror the photo left to right (in the library, the selected photos).", keys: [], when: "any", run: () => turnPhotos({ flip: true }) },
  { id: "keywords", group: tKey("Rating"), label: "Add keywords (jump to the keyword field)", keys: ["K"], when: "any", run: () => keywordsKey() },
  { id: "flag-pick", group: tKey("Rating"), label: "Flag as a pick", keys: ["P"], when: FLAG_CONTEXTS, run: () => flagKey("pick") },
  { id: "flag-reject", group: tKey("Rating"), label: "Flag as rejected", keys: ["X"], when: FLAG_CONTEXTS, run: () => flagKey("reject") },
  { id: "flag-none", group: tKey("Rating"), label: "Remove the flag", keys: ["U"], when: FLAG_CONTEXTS, run: () => flagKey("") },
  { id: "library", group: tKey("Library"), label: "Library grid / editor", hint: "All the photos in the folder, to rate, label, tag and export many at once.", keys: ["G"], when: "any", button: "btn-library", run: () => cullLeave() || libraryToggle() },
  { id: "import", group: tKey("Library"), label: "Import photos from a card or folder", hint: "Copied into dated folders, renamed if you like.", keys: ["Ctrl+Shift+I"], when: "any", run: () => importFlow() },
  { id: "batch-export", group: tKey("File"), label: "Batch export: resize a folder and its subfolders to smaller copies in another folder", hint: "Make lots of photos smaller at once, keeping the folder structure; stop and run again any time.", keys: [], when: "any", run: () => openBatchExport() },
  { id: "styles", group: tKey("Edit"), label: "Styles: looks that fit themselves to each photo, to apply to this photo or the selection", hint: "A style is a look, not fixed moves: it finds each photo's own settings to reach it.", keys: [], when: "any", button: "btn-styles", run: () => stylesOpen().catch(showError) },
  { id: "style-new", group: tKey("Edit"), label: "New style from reference photos…", hint: "Keeps the look of photos you like; Styles… applies it to any photo.", keys: [], when: "any", run: () => stylesNewKey().catch(showError) },
  { id: "convert-raws", group: tKey("File"), label: "Convert raws to JPEGs: the library's selection, or the open photo's folder", keys: [], when: "any", run: () => convertRawsFlow() },
  { id: "save-preset", group: tKey("Edit"), label: "Make a preset from this photo's edit", hint: "Its settings, without the crop, as a one-click preset.", keys: ["Ctrl+Shift+N"], when: "any", photo: true, button: "btn-save-preset", run: () => savePresetFlow() },
  { id: "show-hidden-presets", group: tKey("Edit"), label: "Show hidden presets: hide, unhide or remove presets", hint: "Hidden presets come back dimmed in the Presets tab, to unhide.", keys: [], when: "any", run: () => presetToggleShowHidden() },
  { id: "virtual-copy", group: tKey("Library"), label: "Make a virtual copy", hint: "A second edit of the photo, kept as its own sidecar.", keys: ["Ctrl+'"], when: ["main", "library"], run: () => virtualCopyKey() },
  { id: "quick-collection", group: tKey("Library"), label: "Add to the last collection used", hint: "The selection, or the open photo.", keys: ["B"], when: ["main", "library"], run: () => quickCollectionKey() },
  { id: "ai-denoise", group: tKey("Photo"), label: "AI Denoise: take the noise out of a raw", hint: "The selected raws (or the open one), each into a new DNG beside it, with an AI model on your computer.", keys: [], when: "any", run: () => denoiseFlow().catch(showError) },
  { id: "cleanup", group: tKey("Library"), label: "Clean up: find photos to delete (blurry, copies, eyes closed, black frames…)", hint: "Pages through them all, marked to go; keep any, then they go to the trash together.", keys: [], when: "library", run: () => openCleanup() },
  { id: "best-in-bursts", group: tKey("Library"), label: "Best in each burst: the shot in focus with eyes open", hint: "Marks the best of each burst and dims the rest; then pick it and reject the rest, or stack them.", keys: [], when: "library", run: () => bestInBursts() },
  { id: "lib-loupe", group: tKey("Library"), label: "Loupe: one photo large", hint: "← → step through the photos.", keys: ["E"], when: "library", run: () => setLibraryView("loupe") },
  { id: "lib-compare", group: tKey("Library"), label: "Compare two photos", hint: "← → change the candidate, ↓ swaps, ↑ makes it the select.", keys: ["C"], when: "library", run: () => setLibraryView("compare") },
  { id: "view", group: tKey("View"), label: "View photos full screen, as an image viewer", hint: "From the photo you're on: ← → (or Space, Home, End) or a click on the left or right of the screen step; Esc ends on the photo you got to. Follows JPEG if there is one.", keys: ["V"], when: "any", run: () => viewerFlow() },
  { id: "view-library", group: tKey("Library"), label: "View full screen (F in the library)", hint: "The same viewer as V, on the F key as in other photo apps.", keys: ["F"], when: "library", run: () => viewerFlow() },
  { id: "slideshow", group: tKey("Library"), label: "Slideshow", hint: "The photos shown (or the selected ones), full screen; choose how long each stays, shuffle and the change effect. Space pauses, ← → step, Esc ends.", keys: ["Ctrl+Enter"], when: "library", button: "lib-slideshow", run: () => slideshowFlow() },
  { id: "lib-insights", group: tKey("Library"), label: "Insights: how you shoot", hint: "How often each aperture, focal length, shutter speed, ISO, camera and lens comes up in the photos shown, and how many were kept. Click a bar to show those photos.", keys: ["Shift+I"], when: "library", run: () => setLibraryView("insights") },
  { id: "lib-sort-reverse", group: tKey("Library"), label: "Reverse the sort order", hint: "Flip the bar's sort between ↑ and ↓.", keys: [], when: "library", run: () => libReverseSort() },
  { id: "lib-open", group: tKey("Library"), label: "Open the photo in the editor", keys: ["Enter"], when: "library", run: (e) => (e.target.matches("button, select") ? false : libraryOpenFocused()) },
  { id: "lib-all", group: tKey("Library"), label: "Select all shown", keys: ["Ctrl+A"], when: "library", run: () => librarySelectAll() },
  { id: "lib-none", group: tKey("Library"), label: "Select none", keys: ["Ctrl+D"], when: "library", run: () => librarySelectNone() },
  { id: "lib-stack", group: tKey("Library"), label: "Stack the selection", keys: ["Ctrl+G"], when: "library", run: () => stackSelection() },
  { id: "lib-unstack", group: tKey("Library"), label: "Unstack", keys: ["Ctrl+Shift+G"], when: "library", run: () => unstackSelection() },
  { id: "lib-copy", group: tKey("Library"), label: "Copy the selected photo's settings (in the editor: the open photo's)", keys: ["Ctrl+Shift+C"], when: ["main", "library"], run: () => libraryCopySettings() },
  { id: "lib-paste", group: tKey("Library"), label: "Paste settings onto the selection (in the editor: the filmstrip's)", keys: ["Ctrl+Shift+V"], when: ["main", "library"], run: () => libraryPasteSettings() },
  { id: "lib-leave", group: tKey("Library"), label: "Back to the grid, then the editor", keys: ["Escape"], when: "library", run: () => cullLeave() || leaveLibrary() },
  { id: "slider-next", group: tKey("Sliders"), label: "Select the next slider", hint: "As in Lightroom; the selected one is outlined.", keys: ["."], when: "main", photo: true, run: () => stepSlider(1) },
  { id: "slider-prev", group: tKey("Sliders"), label: "Select the previous slider", keys: [","], when: "main", photo: true, run: () => stepSlider(-1) },
  { id: "slider-up", group: tKey("Sliders"), label: "Raise the selected slider a step", keys: ["="], when: "main", photo: true, run: () => nudgeSlider(1) },
  { id: "slider-down", group: tKey("Sliders"), label: "Lower the selected slider a step", keys: ["-"], when: "main", photo: true, run: () => nudgeSlider(-1) },
  { id: "slider-up-more", group: tKey("Sliders"), label: "Raise the selected slider ten steps", keys: ["Shift++"], when: "main", photo: true, run: () => nudgeSlider(10) },
  { id: "slider-down-more", group: tKey("Sliders"), label: "Lower the selected slider ten steps", keys: ["Shift+_"], when: "main", photo: true, run: () => nudgeSlider(-10) },
  // Zoomed in, the arrows move the photo, also with a tool open; on the
  // whole photo, ← → go to the previous or next photo (not inside a tool).
  { id: "prev-photo", group: tKey("Photos"), label: "Previous photo in the folder", keys: ["ArrowLeft"], when: ["main", "crop", "masks", "spots", "select"], photo: true, run: (e) => (onControl(e) ? false : ui.zoom.on ? panZoom(-0.25, 0) : keyContext() === "main" ? stepPhoto(-1) : false) },
  { id: "next-photo", group: tKey("Photos"), label: "Next photo in the folder", keys: ["ArrowRight"], when: ["main", "crop", "masks", "spots", "select"], photo: true, run: (e) => (onControl(e) ? false : ui.zoom.on ? panZoom(0.25, 0) : keyContext() === "main" ? stepPhoto(1) : false) },
  { id: "pan-up", group: tKey("View"), label: "Move up the zoomed-in photo", keys: ["ArrowUp"], when: ["main", "crop", "masks", "spots", "select"], photo: true, run: (e) => (onControl(e) || !ui.zoom.on ? false : panZoom(0, -0.25)) },
  { id: "pan-down", group: tKey("View"), label: "Move down the zoomed-in photo", keys: ["ArrowDown"], when: ["main", "crop", "masks", "spots", "select"], photo: true, run: (e) => (onControl(e) || !ui.zoom.on ? false : panZoom(0, 0.25)) },
  { id: "fold-left", group: tKey("Window"), label: "Hide or show the left column", keys: ["F7"], when: "any", run: () => toggleColumn("left") },
  { id: "fold-right", group: tKey("Window"), label: "Hide or show the right column", keys: ["F8"], when: "any", run: () => toggleColumn("right") },
  { id: "topbar", group: tKey("Window"), label: "Hide or show the top bar", hint: "Hidden, it comes back while the pointer is at the window's top edge.", keys: ["Ctrl+Shift+T"], when: "any", run: () => setTopbar(document.body.classList.contains("topbar-off")) },
  { id: "zoom-in", group: tKey("View"), label: "Zoom in", hint: "Also the mouse wheel or a pinch, at the pointer. In the library it sizes the thumbnails (or zooms the Map, Loupe and Compare).", keys: ["Ctrl+=", "Ctrl+Shift++", "Ctrl++"], when: ["main", "masks", "spots", "select", "library"], photo: () => !libraryOn(), run: () => (libraryOn() ? mapKeyZoom(1) || cullZoomKey(1) || libZoomKey(1) : zoomKey(1)) },
  { id: "zoom-out", group: tKey("View"), label: "Zoom out", keys: ["Ctrl+-"], when: ["main", "masks", "spots", "select", "library"], photo: () => !libraryOn(), run: () => (libraryOn() ? mapKeyZoom(-1) || cullZoomKey(-1) || libZoomKey(-1) : zoomKey(-1)) },
  { id: "zoom-fit", group: tKey("View"), label: "Fit the whole photo", hint: "In the library: thumbnails back to their usual size.", keys: ["Ctrl+0"], when: ["main", "masks", "spots", "select", "library"], photo: () => !libraryOn(), run: () => (libraryOn() ? cullZoomKey(0) || libZoomKey(0) : setZoom(false)) },
  { id: "peek", group: tKey("View"), label: "Just the photo, filling the screen", hint: "F again (or Esc) comes back. Zoom and move round it as usual.", keys: ["F"], when: ["main", "masks", "spots", "select"], photo: true, run: () => togglePeek() },
  { id: "filmstrip", group: tKey("Photos"), label: "Show or hide the filmstrip", keys: ["F6"], when: ["main", "masks", "spots", "select", "crop"], run: () => toggleFilmstrip() },
  { id: "assistant", group: tKey("Tools"), label: "Assistant: your AI edits the photo", keys: ["Ctrl+Shift+A"], when: "any", photo: true, run: () => assistantFlow() },
  { id: "hub", group: tKey("Tools"), label: "Browse what others shared", hint: "Layouts, themes, presets, collages, plugins and interfaces from the hub.", keys: ["Ctrl+Shift+H"], when: "any", button: "btn-hub", run: () => browseFlow() },
  { id: "plugins", group: tKey("Tools"), label: "Plugins", hint: "Extra tools that make edits; run, install and share them.", keys: ["Ctrl+Shift+P"], when: "any", button: "btn-plugins", run: () => pluginsFlow() },
  { id: "looks", group: tKey("Window"), label: "Appearance: layouts and themes", keys: ["Ctrl+L"], when: "any", button: "btn-looks", run: () => looksFlow() },
  { id: "edit-layout", group: tKey("Window"), label: "Edit layout", hint: "Move panels, controls and toolbar buttons.", keys: ["Ctrl+Shift+L"], when: "any", button: "btn-edit-layout", run: () => startEditing() },
  { id: "blank-layout", group: tKey("Window"), label: "New layout from scratch", hint: "A blank canvas: empty columns and toolbar, filled from Add.", keys: [], when: "any", run: () => startEditing(true) },
  { id: "settings", group: tKey("Window"), label: "Settings and keyboard shortcuts", keys: ["Ctrl+,"], when: "any", button: "btn-settings", run: () => settingsFlow() },
  { id: "feedback", group: tKey("Window"), label: "Send feedback", hint: "A bug, an idea, or a photo that looks different from Lightroom.", keys: [], when: "any", button: "btn-feedback", run: () => feedbackFlow() },
];

// Arrow keys belong to a focused slider, list or knob.
const onControl = (e) => e.target.matches("input, select, textarea, [tabindex], .bar, svg");
// An action's label in the chosen language (`vars` fills its {names}).
const actionLabel = (a) => t(a.label, a.vars);
// "Ctrl+Z or Ctrl+Y"
const keysOr = (list) => (list.length ? list.reduce((a, b) => t("{a} or {b}", { a, b })) : t("no key"));
const keysFor = (a) => ui.keybindings?.[a.id] ?? a.keys;
const contextsOf = (a) => [a.when].flat();
const contextsOverlap = (a, b) => a.when === "any" || b.when === "any" || contextsOf(a).some((w) => contextsOf(b).includes(w));

// The key combination of a keydown event, or null for a lone modifier.
function comboOf(e) {
  if (["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock", "Dead", "Unidentified"].includes(e.key)) return null;
  let key = e.key === " " ? "Space" : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  // Alt on a Mac types special characters (Alt+R is ®); use the key's place instead.
  if (e.altKey && /^Key[A-Z]$|^Digit\d$/.test(e.code) && !/^[A-Z0-9]$/.test(key)) key = e.code.replace(/^Key|^Digit/, "");
  const parts = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  return [...parts, key].join("+");
}

const KEY_NAMES = { Escape: "Esc", ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Delete: "Del", Backspace: "⌫", Enter: "Enter", Space: "Space" };

// How a key looks on screen: "Ctrl+Shift+Z", or "⌘⇧Z" on a Mac.
function keyLabel(combo) {
  const parts = combo.split("+");
  // "Ctrl++" is Ctrl and the plus key.
  const key = combo.endsWith("++") ? "+" : parts.pop();
  const mods = combo.endsWith("++") ? parts.filter((p) => p) : parts;
  const name = KEY_NAMES[key] ?? key;
  if (IS_MAC) {
    const sym = { Ctrl: "⌘", Alt: "⌥", Shift: "⇧" };
    return mods.map((m) => sym[m] ?? m).join("") + name;
  }
  return [...mods, name].join("+");
}

// Which actions' keys work now (an action's `when`).
const keyContext = () => (libraryOn() ? "library" : ui.crop.on ? "crop" : mk.on ? "masks" : sp.on ? "spots" : sl.on ? "select" : "main");

// When each action last ran from its key, so a menu bar that saw the key too
// doesn't run it again (menu.js).
const keyRunAt = {};

// Whether an action waits for an open photo: `photo` is true, or a function
// (the zoom keys need one outside the library only).
const needsPhoto = (a) => (typeof a.photo === "function" ? a.photo() : Boolean(a.photo));

function dispatchKey(e) {
  if (e.target.matches("input[type=number], input[type=search], input[type=text], textarea, input.value")) return;
  // Keys belong to an open dialog.
  if (document.querySelector("dialog[open]")) return;
  const combo = comboOf(e);
  if (!combo) return;
  const ctx = keyContext();
  for (const a of KEY_ACTIONS) {
    if (!keysFor(a).includes(combo)) continue;
    if (a.when !== "any" && !contextsOf(a).includes(ctx)) continue;
    if (needsPhoto(a) && !ui.info) continue;
    if (a.run(e) === false) continue;
    keyRunAt[a.id] = Date.now();
    e.preventDefault();
    return;
  }
}

// Tooltips on the toolbar buttons name their current keys.
function updateKeyHints() {
  for (const a of KEY_ACTIONS) {
    const b = a.button && $(a.button);
    if (!b) continue;
    const keys = keysFor(a).map(keyLabel);
    const named = keys.length ? t("{text} ({keys})", { text: actionLabel(a), keys: keysOr(keys) }) : actionLabel(a);
    b.title = a.hint ? t("{text}\n{hint}", { text: named, hint: t(a.hint) }) : named;
  }
  buildMenu();
}

async function loadKeybindings() {
  try {
    ui.keybindings = await invoke("keybindings");
  } catch (e) {
    ui.keybindings = {};
    toast(`Couldn't load your keyboard shortcuts: ${e}`, true);
  }
  updateKeyHints();
}

async function saveKeybindings() {
  // Only changes are stored; an action back at its defaults is dropped.
  for (const a of KEY_ACTIONS) {
    const mine = ui.keybindings[a.id];
    if (mine && mine.length === a.keys.length && mine.every((k, i) => k === a.keys[i])) delete ui.keybindings[a.id];
  }
  updateKeyHints();
  try {
    await invoke("set_keybindings", { bindings: ui.keybindings });
  } catch (e) {
    showError(e);
  }
}

// ---------------------------------------------------------------- the Settings dialog

const KEYBOARD_ROWS = [
  ["Escape", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="],
  ["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "\\"],
  ["A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'", "Enter"],
  ["Z", "X", "C", "V", "B", "N", "M", ",", ".", "/"],
  ["Space"],
];

// The Shortcuts tab's functions take its state `ks` (made in settingsFlow):
// `capturing` is the action waiting for a new key, `pending` a key that clashes,
// waiting for "use it here", `focusKey` a key picked on the keyboard map.
const keysBase = (combo) => (combo.endsWith("++") ? "+" : combo.split("+").pop());

function keysMessage(text, actions = []) {
  const box = $("keys-message");
  box.replaceChildren(document.createTextNode(text), ...actions);
}

function keysRenderMap(ks) {
  const map = $("keyboard-map");
  map.replaceChildren();
  for (const row of KEYBOARD_ROWS) {
    const r = document.createElement("div");
    r.className = "kb-row";
    for (const key of row) {
      const users = KEY_ACTIONS.flatMap((a) => keysFor(a).filter((k) => keysBase(k) === key).map((k) => [k, a]));
      const cap = document.createElement("button");
      cap.type = "button";
      cap.className = `kb-key kb-${key.length > 1 ? key.toLowerCase() : "char"}`;
      cap.textContent = KEY_NAMES[key] ?? key;
      cap.classList.toggle("bare", users.some(([k]) => k === key));
      cap.classList.toggle("with-mod", users.some(([k]) => k !== key));
      cap.classList.toggle("focus", ks.focusKey === key);
      cap.title = users.length ? users.map(([k, a]) => t("{key}: {action}", { key: keyLabel(k), action: actionLabel(a) })).join("\n") : t("{key}: free", { key: KEY_NAMES[key] ?? key });
      cap.addEventListener("click", () => {
        ks.focusKey = ks.focusKey === key ? null : key;
        keysRender(ks);
      });
      r.append(cap);
    }
    map.append(r);
  }
}

function keysRenderList(ks) {
  const filter = $("keys-filter").value.trim().toLowerCase();
  const list = $("keys-list");
  list.replaceChildren();
  let group = null;
  for (const a of KEY_ACTIONS) {
    const keys = keysFor(a);
    if (ks.focusKey && !keys.some((k) => keysBase(k) === ks.focusKey)) continue;
    if (filter && !`${actionLabel(a)} ${t(a.group)} ${keys.map(keyLabel).join(" ")}`.toLowerCase().includes(filter)) continue;
    if (a.group !== group) {
      group = a.group;
      list.append(el("h3", {}, t(group)));
    }
    const row = document.createElement("div");
    row.className = "key-row";
    const name = document.createElement("span");
    name.className = "key-action";
    name.textContent = actionLabel(a);
    if (a.when === "crop") name.title = t("Only while the crop tool is open");
    if (a.when === "masks") name.title = t("Only while the masks are open");
    if (a.when === "spots") name.title = t("Only while spot removal is open");
    if (a.when === "library") name.title = t("Only in the library");
    const caps = document.createElement("span");
    caps.className = "key-caps";
    for (const k of keys) {
      const kbd = document.createElement("kbd");
      kbd.textContent = keyLabel(k);
      const x = el("button", { type: "button", class: "kbd-remove", title: t("Remove {key}", { key: keyLabel(k) }) }, "×");
      x.addEventListener("click", () => {
        ui.keybindings[a.id] = keys.filter((y) => y !== k);
        saveKeybindings();
        keysRender(ks);
      });
      kbd.append(x);
      caps.append(kbd);
    }
    if (ks.capturing === a.id) {
      caps.append(el("span", { class: "key-capture" }, t("Press the new shortcut… (Esc cancels)")));
    } else if (!keys.length) {
      caps.append(el("span", { class: "hint" }, t("none")));
    }
    const add = el("button", { type: "button", class: "small", disabled: keys.length >= 4 }, t("Add key"));
    add.addEventListener("click", () => {
      ks.capturing = a.id;
      ks.pending = null;
      keysMessage("");
      keysRender(ks);
    });
    const changed = Boolean(ui.keybindings[a.id]);
    const reset = el("button", { type: "button", class: "small", disabled: !changed, title: t("Back to {keys}", { keys: keysOr(a.keys.map(keyLabel)) }) }, t("Reset"));
    reset.addEventListener("click", () => {
      delete ui.keybindings[a.id];
      saveKeybindings();
      keysRender(ks);
    });
    row.classList.toggle("changed", changed);
    row.classList.toggle("capturing", ks.capturing === a.id);
    row.append(name, caps, add, reset);
    list.append(row);
  }
  if (!list.children.length) list.append(el("p", { class: "hint" }, t("No shortcuts match.")));
}

function keysRender(ks) {
  keysRenderMap(ks);
  keysRenderList(ks);
}

// Give `combo` to `action`; when another action has it, ask first.
function keysAssign(ks, action, combo) {
  const clashes = KEY_ACTIONS.filter((b) => b !== action && contextsOverlap(action, b) && keysFor(b).includes(combo));
  const add = () => {
    for (const b of clashes) ui.keybindings[b.id] = keysFor(b).filter((k) => k !== combo);
    if (!keysFor(action).includes(combo)) ui.keybindings[action.id] = [...keysFor(action), combo];
    ks.capturing = ks.pending = null;
    keysMessage(t("{key} now does \"{action}\".", { key: keyLabel(combo), action: actionLabel(action) }));
    saveKeybindings();
    keysRender(ks);
  };
  if (!clashes.length) return add();
  ks.pending = combo;
  ks.capturing = null;
  const use = el("button", { type: "button", class: "small primary" }, t("Use it here instead"));
  use.addEventListener("click", add);
  const keep = el("button", { type: "button", class: "small" }, t("Keep it there"));
  keep.addEventListener("click", () => { ks.pending = null; keysMessage(""); keysRender(ks); });
  keysMessage(t("{key} already does \"{actions}\". ", { key: keyLabel(combo), actions: clashes.map(actionLabel).join('", "') }), [use, " ", keep]);
  keysRender(ks);
}

function settingsShowTab(dlg, name) {
  dlg.querySelectorAll(".settings-tabs button").forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
  $("settings-keys").hidden = name !== "keys";
  $("settings-general").hidden = name !== "general";
  $("settings-about").hidden = name !== "about";
  $("settings-models").hidden = name !== "models";
  $("keys-reset-all").hidden = name !== "keys";
  if (name === "models") modelsTab().catch(showError);
}

async function settingsSetHub(url) {
  try {
    $("settings-hub").value = await invoke("set_hub_url", { url });
  } catch (e) {
    showError(e);
  }
}

// Edit in another app.
function settingsShowEditIn(e) {
  $("settings-edit-app").value = e.app ?? "";
  $("settings-edit-space").value = e.color_space;
  $("settings-edit-bits").value = String(e.bits);
}

async function settingsSaveEditIn(app) {
  try {
    settingsShowEditIn(await invoke("set_edit_in", {
      settings: {
        app: app === undefined ? $("settings-edit-app").value || null : app,
        color_space: $("settings-edit-space").value,
        bits: Number($("settings-edit-bits").value),
      },
    }));
  } catch (e) {
    showError(e);
  }
}

// The licence or notice text, shown in the About tab.
const settingsShowText = (which) => async () => {
  try {
    const pre = $("about-text");
    pre.textContent = await invoke("about_text", { which });
    pre.hidden = false;
    pre.scrollTop = 0;
  } catch (e) {
    showError(e);
  }
};

async function settingsFlow() {
  const dlg = $("settings-dialog");
  const ks = { capturing: null, pending: null, focusKey: null };

  // While waiting for a new key, every key goes to the dialog.
  const onKey = (e) => {
    if (!ks.capturing) return;
    e.preventDefault();
    e.stopPropagation();
    const combo = comboOf(e);
    if (!combo) return;
    if (combo === "Escape") {
      ks.capturing = null;
      return keysRender(ks);
    }
    if (["Tab", "Shift+Tab"].includes(combo)) return keysMessage(t("Tab moves between controls, so it can't be a shortcut."));
    const action = KEY_ACTIONS.find((a) => a.id === ks.capturing);
    keysAssign(ks, action, combo);
  };
  const onCancel = (e) => { if (ks.capturing) e.preventDefault(); };
  const tabs = [...dlg.querySelectorAll(".settings-tabs button")];
  const onTab = (e) => settingsShowTab(dlg, e.currentTarget.dataset.tab);
  const resetAll = () => {
    ui.keybindings = {};
    ks.capturing = ks.pending = null;
    keysMessage(t("All shortcuts are back to their defaults."));
    saveKeybindings();
    keysRender(ks);
  };
  const onFilter = () => keysRenderList(ks);
  const openFolder = () => invoke("open_settings_folder").catch(showError);
  const onHub = () => settingsSetHub($("settings-hub").value);
  const pickEditApp = async () => {
    const app = await invoke("pick_edit_app");
    if (app) settingsSaveEditIn(app);
  };
  const editDefault = () => settingsSaveEditIn(null);
  const onEditFormat = () => settingsSaveEditIn();
  const hubDefault = () => settingsSetHub(null);
  const openLooks = () => { dlg.close(); setTimeout(() => looksFlow().catch(showError)); };
  const onLicence = settingsShowText("licence");
  const onNotice = settingsShowText("notice");
  const onSource = () => invoke("open_source_page").catch(showError);

  window.addEventListener("keydown", onKey, true);
  dlg.addEventListener("cancel", onCancel);
  tabs.forEach((t) => t.addEventListener("click", onTab));
  $("keys-reset-all").addEventListener("click", resetAll);
  $("keys-filter").addEventListener("input", onFilter);
  $("settings-folder").addEventListener("click", openFolder);
  $("settings-looks").addEventListener("click", openLooks);
  $("settings-hub").addEventListener("change", onHub);
  $("settings-hub-default").addEventListener("click", hubDefault);
  $("settings-edit-pick").addEventListener("click", pickEditApp);
  $("settings-edit-default").addEventListener("click", editDefault);
  $("settings-edit-space").addEventListener("change", onEditFormat);
  $("settings-edit-bits").addEventListener("change", onEditFormat);
  $("about-licence").addEventListener("click", onLicence);
  $("about-notice").addEventListener("click", onNotice);
  $("about-source").addEventListener("click", onSource);
  $("about-text").hidden = true;
  invoke("hub_url").then((u) => ($("settings-hub").value = u)).catch(() => {});
  invoke("edit_in_settings").then(settingsShowEditIn).catch(() => {});
  renderInterfaces();

  try {
    const info = await invoke("settings_info");
    $("settings-version").textContent = `Ambrotype ${info.version}`;
    $("about-version").textContent = info.version;
    $("settings-path").textContent = info.folder ?? "";
  } catch { /* the General tab just shows less */ }
  $("keys-filter").value = "";
  keysMessage("");
  settingsShowTab(dlg, "keys");
  keysRender(ks);
  dlg.showModal();
  await new Promise((resolve) => dlg.addEventListener("close", resolve, { once: true }));

  window.removeEventListener("keydown", onKey, true);
  dlg.removeEventListener("cancel", onCancel);
  tabs.forEach((t) => t.removeEventListener("click", onTab));
  $("keys-reset-all").removeEventListener("click", resetAll);
  $("keys-filter").removeEventListener("input", onFilter);
  $("settings-folder").removeEventListener("click", openFolder);
  $("settings-looks").removeEventListener("click", openLooks);
  $("settings-hub").removeEventListener("change", onHub);
  $("settings-hub-default").removeEventListener("click", hubDefault);
  $("settings-edit-pick").removeEventListener("click", pickEditApp);
  $("settings-edit-default").removeEventListener("click", editDefault);
  $("settings-edit-space").removeEventListener("change", onEditFormat);
  $("settings-edit-bits").removeEventListener("change", onEditFormat);
  $("about-licence").removeEventListener("click", onLicence);
  $("about-notice").removeEventListener("click", onNotice);
  $("about-source").removeEventListener("click", onSource);
}

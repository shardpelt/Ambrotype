// SPDX-License-Identifier: GPL-3.0-or-later
// Opening photos, the folder's photos, the film base, the lens database,
// saving, and before and after. Split out of app.js.
"use strict";

// ---------------------------------------------------------------- photo

// A folder of photos opens in the library (chosen, or dropped on the window).
// One at a time: a slow folder (a memory card) can take a while to answer,
// and a second click meanwhile would only ask again.
async function openFolder(dir) {
  if (ui.openingFolder) {
    toast("Still opening the folder…");
    return false;
  }
  ui.openingFolder = true;
  $("btn-open-folder-empty").disabled = true;
  $("busy").hidden = false;
  try {
    if (!dir) {
      status("Choosing a folder…", true);
      dir = await invoke("library_pick_folder");
    }
    if (!dir) return false;
    status(`Opening ${dir}…`, true);
    showReading(dir);
    if (!libraryOn()) await switchTask("library");
    await loadLibrary(dir, []);
    return true;
  } finally {
    ui.openingFolder = false;
    $("btn-open-folder-empty").disabled = false;
    $("busy").hidden = true;
    status("Ready", true);
  }
}

async function openPath(path) {
  status(`Opening ${path}…`, true);
  $("busy").hidden = false;
  try {
    const r = await invoke("open_photo", { path });
    if (ui.crop.on) {
      setCropMode(false);
      ui.crop.tool.stop();
    }
    masksPhotoChanged();
    spotsPhotoChanged();
    const wasRendered = !!ui.info?.rendered;
    ui.info = r.info;
    framePhotoChanged();
    refreshProfiles();
    refreshLuts();
    // Raw and JPEG have different white balance sliders.
    if (wasRendered !== !!r.info.rendered && ui.layout) applyLayout(ui.layout);
    $("empty").hidden = true;
    $("photo-title").textContent = `${r.info.name}  —  ${r.info.camera}${r.info.lens ? "  ·  " + r.info.lens : ""}`;
    $("exif").textContent = [r.info.exposure, `${r.info.width} × ${r.info.height}`,
      r.info.sidecar_loaded ? "edits loaded from sidecar" : ""].filter(Boolean).join("  ·  ");
    for (const id of ["btn-save", "btn-export", "btn-before", "btn-crop", "btn-masks", "btn-spots", "btn-zoom", "btn-wb", "btn-clip", "btn-frame", "btn-auto", "btn-select", "btn-save-preset", "btn-match", "btn-surprise"]) $(id).disabled = false;
    if (ui.zoom.on) setZoom(false);
    setState(r.state);
    if (r.saved_previous) toast(`Saved your edits to ${fileName(r.saved_previous)}`);
    applyStripVisibility();
    loadFolder(r.info.path);
    requestPreview(false);
    loadInfo();
    offerLensDb();
    return true;
  } catch (e) {
    showError(e);
    status("Ready");
    return false;
  } finally {
    $("busy").hidden = true;
  }
}

// A photo, or a saved collage (.ambcollage).
function openAny(path) {
  return /\.ambcollage$/i.test(path) ? openCollageFile(path) : openPath(path);
}

async function openDialog() {
  const path = await invoke("pick_photo");
  if (path) openPath(path);
}

// A small right-click menu: items are [label, action] (null for a line).
function contextMenu(e, items) {
  e.preventDefault();
  document.querySelector(".ctx-menu")?.remove();
  const menu = Object.assign(document.createElement("div"), { className: "ctx-menu", role: "menu" });
  // The button's tooltip would cover the menu.
  const owner = e.currentTarget instanceof Element ? e.currentTarget : null;
  const title = owner?.getAttribute("title");
  if (title) owner.removeAttribute("title");
  const close = () => {
    if (title) owner.setAttribute("title", title);
    menu.remove();
    removeEventListener("pointerdown", outside, true);
    removeEventListener("keydown", esc, true);
  };
  const outside = (ev) => { if (!menu.contains(ev.target)) close(); };
  const esc = (ev) => { if (ev.key === "Escape") { ev.stopPropagation(); close(); } };
  for (const item of items) {
    if (!item) { menu.append(document.createElement("hr")); continue; }
    const [label, action] = item;
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: label, role: "menuitem" });
    b.addEventListener("click", () => { close(); Promise.resolve(action()).catch(showError); });
    menu.append(b);
  }
  document.body.append(menu);
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(e.clientX, innerWidth - r.width - 4)}px`;
  menu.style.top = `${Math.min(e.clientY, innerHeight - r.height - 4)}px`;
  addEventListener("pointerdown", outside, true);
  addEventListener("keydown", esc, true);
}

// The photos in the open photo's folder (the filmstrip's list).
function folderPhotos() {
  return (strip.photos ?? []).map((p) => p.path);
}

// Apply a preset to every photo in the folder (the open one as one undo step).
async function freezeNegative() {
  if (!ui.info) return;
  try {
    run(await invoke("negative_freeze"), "Freeze negative analysis");
  } catch (e) {
    showError(e);
  }
}

// Pick a negative's film base on the photo; `measure`: forget a picked one,
// so it is measured from the scan again.
function pickBaseFlow(measure = false) {
  if (!ui.info) return;
  if (measure) {
    run(["Red", "Green", "Blue"].map((c) => ({ cmd: "reset_param", name: `amb:FilmBase${c}` })), "Measure film base");
    return;
  }
  setPicking(ui.picking === "base" ? false : "base");
}

async function cropToFrame() {
  if (!ui.info) return;
  try {
    const cmds = await invoke("negative_frame");
    if (!cmds) return toast("No film border found around the photo.");
    run(cmds, "Crop to frame");
  } catch (e) {
    showError(e);
  }
}

// Once, when no lens profiles are found at all: offer the download.
let lensDbOffered = false;
// No lens profiles at all (no lensfun installed, none downloaded): fetch
// lensfun's database quietly (about 0.5 MB), as the small Subject model is,
// rather than asking in a dialog before the first photo is even seen. Once
// per computer; Lens Corrections → Lens database… updates it.
async function offerLensDb() {
  if (lensDbOffered) return;
  lensDbOffered = true;
  if (store.text("lensDbOffered")) return;
  // The profiles load in the background at start.
  let st = null;
  for (let i = 0; i < 30; i++) {
    try {
      st = await invoke("lens_db_state");
    } catch {
      return;
    }
    if (st.lenses !== null) break;
    await new Promise((r) => setTimeout(r, 1000));
  }
  if (!st || st.lenses !== 0 || st.testing) return;
  store.setText("lensDbOffered", "1");
  try {
    const r = await invoke("fetch_lens_db");
    if (r.lenses) {
      toast(`Downloaded lens profiles (lensfun, ${r.lenses} lenses): Lens Corrections can now fix distortion and vignetting.`);
      requestPreview(false);
    }
  } catch {
    // Offline: Lens Corrections → Lens database… can fetch it later.
  }
}

// lensfun's database: download it (or update it) and load it.
async function lensDbFlow() {
  let st;
  try {
    st = await invoke("lens_db_state");
  } catch (e) {
    return showError(e);
  }
  const when = st.timestamp ? new Date(st.timestamp * 1000).toLocaleDateString() : null;
  const have = st.lenses === null ? "The lens profiles are still loading." : `${st.lenses} lens profiles are loaded.`;
  const ask = when
    ? `${have} The downloaded database is from ${when}. Check for a newer one?`
    : `${have} Download lensfun's lens database (about 0.5 MB, CC BY-SA 3.0) into ${st.dir}?`;
  if (!await askYes(ask)) return;
  lensDbFetch();
}

async function lensDbFetch() {
  status("Downloading the lens database…");
  $("busy").hidden = false;
  try {
    const r = await invoke("fetch_lens_db");
    toast(`Lens database ready: ${r.lenses ?? 0} lens profiles`);
    status("Lens database ready");
    requestPreview(false);
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
  }
}

async function save() {
  if (!ui.info) return;
  try {
    const path = await invoke("save_sidecar");
    toast(`Saved to ${path}`);
  } catch (e) {
    showError(e);
  }
}

// Before / after (\\): the split with its bar near the right edge, so the
// photo shows as it was and the bar drags in to show the edit; again to
// leave. Zoomed in or cropping, where the split can't show, the whole
// photo as it was instead.
function toggleBefore() {
  if (!ui.info) return;
  if (ui.before) {
    ui.before = false;
  } else if (ui.split) {
    toggleSplit();
  } else if (ui.zoom.on || ui.crop.on) {
    ui.before = true;
  } else {
    splitAt = 0.92;
    toggleSplit();
  }
  $("before-badge").hidden = !ui.before;
  $("btn-before").classList.toggle("active", ui.before || ui.split);
  requestPreview(false);
}

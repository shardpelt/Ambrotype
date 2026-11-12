// SPDX-License-Identifier: GPL-3.0-or-later
// The library's actions on many photos: open, copy and paste an edit, export,
// and start a collage. Split out of library.js.
"use strict";

// ---------------------------------------------------------------- open, copy and paste, export, collage

async function libraryOpen(path) {
  if (!path) return;
  if (libPhoto(path)?.offline) return toast("This photo's drive isn't plugged in. Plug it in to open the photo.", true);
  await toggleLibrary(false);
  if (path !== ui.info?.path) await openPath(path);
}

// Leaving the library for the editor (a task, G, Escape): the editor shows
// the library's photo, as Lightroom's Develop does. That's the focused or
// selected photo, else the open one if the library shows it, else the first
// one shown; with nothing shown, the editor stays as it was.
function leaveLibrary() {
  const shown = (path) => path && lib.shown.some((p) => p.path === path);
  const pick = [lib.focus, selectedPhotos()[0]?.path, ui.info?.path, lib.shown[0]?.path].find(shown);
  if (!pick || libPhoto(pick)?.offline) return toggleLibrary(false);
  return libraryOpen(pick);
}

// G and the Library button: into the library, or out of it to its photo.
function libraryToggle() {
  return libraryOn() ? leaveLibrary() : toggleLibrary(true);
}

function libraryOpenFocused() {
  return libraryOpen((cullOn() && cull.active) || lib.focus || selectedPhotos()[0]?.path);
}

async function libraryCopySettings() {
  if (!lib.on) {
    // The editor: the open photo's edit, as it is now.
    if (!ui.info) return;
    await invoke("save_sidecar").catch(() => {});
    lib.copied = ui.info.path;
    return toast(`Copied the settings of ${fileName(ui.info.path)}: select photos in the filmstrip (Ctrl- or Shift-click), then Paste settings`);
  }
  const sel = selectedPhotos();
  if (sel.length !== 1) return toast("Select the one photo to copy the settings from.", true);
  lib.copied = sel[0].path;
  toast(`Copied the settings of ${sel[0].name}`);
  renderPanel();
}

// A virtual copy of one photo: a second edit, kept as its own sidecar.
async function makeVirtualCopy(path) {
  try {
    const copy = await invoke("create_virtual_copy", { path });
    status(`Made a virtual copy of ${fileName(path)}`);
    return copy;
  } catch (e) {
    toast(String(e), true);
    return null;
  }
}

// Ctrl+': in the library, of the selected photo; in the editor, of the open
// photo, which then opens.
async function virtualCopyKey() {
  if (lib.on) {
    const sel = libraryTargets();
    if (sel.length !== 1) return toast("Select the one photo to make a virtual copy of.", true);
    const copy = await makeVirtualCopy(sel[0].path);
    if (!copy) return;
    if (lib.collection) await addToCollection([copy], lib.collection);
    else await loadLibrary(lib.dir);
    select(copy, "only");
    revealPhoto(copy);
    return;
  }
  if (!ui.info) return false;
  const copy = await makeVirtualCopy(ui.info.path);
  if (copy) await openPath(copy);
}

async function libraryDeleteCopies() {
  const copies = libraryTargets().filter((p) => p.copy);
  if (!copies.length) return;
  const what = copies.length === 1 ? copies[0].name : `${copies.length} virtual copies`;
  if (!await askYes(`Delete ${what}? Only the copy's edit goes; the photo stays.`)) return;
  // The editor can't keep a deleted copy open: switch to its photo first.
  const open = copies.find((p) => p.path === ui.info?.path);
  if (open) {
    const master = lib.photos.find((p) => !p.copy && open.name.startsWith(`${p.name} ·`));
    if (master) await openPath(master.path);
  }
  let failed = 0;
  for (const p of copies) {
    try {
      await invoke("delete_virtual_copy", { path: p.path });
    } catch (e) {
      failed++;
      toast(String(e), true);
    }
  }
  status(`Deleted ${copies.length - failed} virtual cop${copies.length - failed === 1 ? "y" : "ies"}`);
  lib.selected.clear();
  if (lib.smart) await showSmartCollection(lib.smart);
  else if (lib.collection) await showCollection(lib.collection);
  else await loadLibrary(lib.dir);
}

// One conversion for a roll of negatives (roll baseline): each scene's
// median, odd frames their own, or the focused frame as the baseline.
// Contact sheets of the selected photos, saved where the user picks
// (src/commands/batch.rs `contact_sheet`).
async function contactSheetFlow() {
  const paths = selectedPhotos().map((p) => p.path);
  if (!paths.length) return toast("Select the photos for the sheet.", true);
  const dlg = $("contact-dialog");
  $("contact-what").textContent = `${paths.length} photo${paths.length === 1 ? "" : "s"}, in the order shown.`;
  const style = $("contact-style");
  style.onchange = () => { $("contact-columns").value = style.value === "film" ? 6 : 5; };
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const options = {
    style: style.value,
    columns: Number($("contact-columns").value) || 6,
    paper: $("contact-paper").value,
    landscape: $("contact-landscape").checked,
    title: $("contact-title").value,
    format: $("contact-format").value,
  };
  status(`Making a contact sheet of ${paths.length} photos…`);
  $("busy").hidden = false;
  try {
    const pages = await invoke("contact_sheet", { paths, options, dir: lastExportDir() });
    if (pages?.length) {
      rememberExportDir(pages[0]);
      toast(pages.length === 1 ? `Saved the contact sheet: ${pages[0]}` : `Saved ${pages.length} pages of contact sheets in ${dirOf(pages[0])}`);
    }
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
    status("Ready");
  }
}

// Smart search over the photos the library has open: by the search box's
// words, or (`like`) for photos like one (src/commands/models.rs `smart_search`).
async function smartSearch(like = null) {
  const query = $("lib-search").value.trim();
  if (!like && !query) return toast("Type what to look for, then press Smart (a dog on a beach, snowy mountains).", true);
  const paths = lib.photos.map((p) => p.path);
  status(like ? "Looking for photos like it…" : `Looking for “${query}”…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("smart_search", { paths, query: like ? null : query, like });
    lib.found = {
      of: lib.photos,
      rank: new Map(r.found.map(([p], i) => [p, i])),
      label: like ? `Like ${fileName(like)}` : `“${query}”`,
    };
    const chip = $("lib-smart-chip");
    chip.textContent = `${lib.found.label}: ${r.found.length} found ×`;
    chip.hidden = false;
    renderLibrary();
    status(`${r.found.length} found with ${r.model}${r.indexed ? ` (${r.indexed} photos indexed first)` : ""}`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
  }
}

function clearSmartSearch() {
  lib.found = null;
  $("lib-smart-chip").hidden = true;
  renderLibrary();
}

async function syncNegativeRoll() {
  const paths = selectedPhotos().map((p) => p.path);
  if (paths.length < 2) return toast("Select the frames of the roll.", true);
  const dlg = $("roll-dialog");
  const focus = paths.includes(lib.focus) ? lib.focus : null;
  $("roll-what").textContent = `${paths.length} frames. Each is analysed; the roll then shares one film base and levels, with Auto off, so a frame exposed a stop more prints a stop brighter.`;
  $("roll-baseline").disabled = !focus;
  $("roll-baseline").checked = false;
  $("roll-baseline-label").textContent = focus ? `Use ${fileName(focus)} as the baseline` : "Use this frame as the baseline (click one first)";
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const options = {
    scenes: $("roll-scenes").checked,
    odd: $("roll-odd").checked,
    own_midtone: $("roll-own-midtone").checked,
    baseline: $("roll-baseline").checked && focus ? paths.indexOf(focus) : null,
  };
  status(`Analysing ${paths.length} negatives…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("negative_sync", { paths, options });
    for (const path of r.changed) {
      const p = libPhoto(path);
      if (p) {
        p.edited = true;
        updateCell(p);
        stripUpdate(path, { edited: true });
      }
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    const parts = [];
    if (r.scenes > 1) parts.push(`${r.scenes} scenes`);
    if (r.odd > 0) parts.push(`${r.odd} odd frame${r.odd === 1 ? "" : "s"} kept their own`);
    toast(`Synced ${r.changed.length} negative${r.changed.length === 1 ? "" : "s"}${parts.length ? ` (${parts.join(", ")})` : ""}${r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : ""}`, r.errors.length > 0);
    status(`One conversion for ${r.changed.length} frames`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
  }
}

// Raws to JPEGs (src/commands/batch.rs `convert_raws`): in the library
// the selection, or every photo shown when none is selected; in the editor
// the open photo's folder (the filmstrip), like the open photo.
async function convertRawsFlow() {
  let paths, what, like;
  if (lib.on) {
    const sel = selectedPhotos();
    paths = (sel.length ? sel : lib.shown).map((p) => p.path);
    what = sel.length ? " selected" : " shown";
    like = lib.focus && paths.includes(lib.focus) ? lib.focus : null;
  } else {
    if (!ui.info) return toast("Open a photo first.", true);
    // Its edit as it is now, for "like this photo" and for itself.
    await invoke("save_sidecar").catch(() => {});
    const picked = stripSelected();
    paths = (picked.length ? picked : strip.photos?.length ? strip.photos : [{ path: ui.info.path }]).map((p) => p.path);
    what = picked.length ? " selected in the filmstrip" : " in this folder";
    like = ui.info.path;
  }
  if (!paths.length) return;
  const dlg = $("convert-dialog");
  const f = dlg.querySelector("form");
  $("convert-what").textContent = `${paths.length} photo${paths.length === 1 ? "" : "s"}${what}.`;
  // Which photo to match: the edited ones among them (the focused or open
  // one first), to pick from.
  const info = (path) => (lib.on ? libPhoto(path) : strip.photos?.find((p) => p.path === path)) ?? { path };
  const edited = paths.filter((p) => p !== like && info(p).edited);
  const choices = [...(like ? [like] : []), ...edited].slice(0, 30);
  if (!like && choices.length) like = choices[0];
  const showLike = () => {
    $("convert-like").disabled = !like;
    $("convert-like-label").innerHTML = like
      ? `<b>Like ${fileName(like)}</b>: every photo matched to it${choices.length > 1 ? " (pick another below)" : ""}`
      : "<b>Like one I edited</b>: edit one of them first";
    for (const b of $("convert-like-pick").children) b.classList.toggle("on", b.dataset.path === like);
  };
  $("convert-like-pick").replaceChildren(...(choices.length > 1 ? choices : []).map((path) => {
    const b = Object.assign(document.createElement("button"), { type: "button", title: `Match every photo to ${fileName(path)}` });
    b.dataset.path = path;
    const img = Object.assign(document.createElement("img"), { alt: fileName(path) });
    bigImage(info(path), 128, true).then((u) => { img.src = u; }).catch(() => {});
    b.append(img);
    b.addEventListener("click", () => {
      like = path;
      f.convert_look.value = "like";
      showLike();
    });
    return b;
  }));
  showLike();
  if (!like && f.convert_look.value === "like") f.convert_look.value = "camera";
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const options = {
    look: f.convert_look.value,
    like,
    long_edge: Number($("convert-size").value),
    quality: Number($("convert-quality").value) || 100,
    edits: $("convert-edits").checked,
    best_of_bursts: $("convert-bursts").checked,
  };
  status(`Converting ${paths.length} raws…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("convert_raws", { paths, options });
    if (!r) return;
    for (const path of paths) refreshThumb(path);
    const parts = [`${r.written.length} JPEG${r.written.length === 1 ? "" : "s"} written`];
    if (r.camera_jpegs) parts.push(`${r.camera_jpegs} as the camera saved ${r.camera_jpegs === 1 ? "it" : "them"}`);
    if (r.skipped) parts.push(`${r.skipped} already there`);
    if (r.left_out) parts.push(`${r.left_out} left out (not the best of their burst)`);
    if (r.errors.length) parts.push(`${r.errors.length} failed: ${r.errors[0]}`);
    toast(parts.join("; "), r.errors.length > 0);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
    status("Ready");
  }
}

// Pick a preset, then apply it to `paths` (the library's selection), each
// photo one undoable step (src/commands/presets.rs `apply_to_photos`).
async function presetsOnPhotos(paths) {
  if (!paths.length) return;
  if (!ui.presets?.length) await loadPresets();
  const dlg = $("preset-pick-dialog");
  const filter = $("preset-pick-filter");
  const amount = $("preset-pick-amount");
  $("preset-pick-title").textContent = paths.length > 1 ? `Apply a preset to ${paths.length} photos` : `Apply a preset to ${fileName(paths[0])}`;
  amount.oninput = () => { $("preset-pick-amount-out").value = amount.value; };
  amount.oninput();
  let chosen = null;
  const render = () => {
    const q = filter.value.trim().toLowerCase();
    const shown = presetsShown().filter((p) => !q || `${p.group} ${p.name}`.toLowerCase().includes(q));
    $("preset-pick-list").replaceChildren(...shown.map((p) => {
      const b = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: p.name, title: p.group });
      b.addEventListener("click", () => { chosen = p; dlg.close("ok"); });
      return b;
    }));
    if (!shown.length) $("preset-pick-list").append(Object.assign(document.createElement("p"), { className: "hint", textContent: "No presets match." }));
  };
  filter.value = "";
  filter.oninput = render;
  render();
  dlg.showModal();
  filter.focus();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok" || !chosen) return;
  status(`Applying "${chosen.name}" to ${paths.length} photos…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("apply_to_photos", {
      paths,
      label: `preset ${chosen.name}`,
      commands: await presetCommands(chosen.path, { amount: Number(amount.value) }),
    });
    for (const path of r.changed) {
      const p = libPhoto(path);
      if (p) {
        p.edited = true;
        updateCell(p);
        stripUpdate(path, { edited: true });
      }
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    toast(`"${chosen.name}" applied to ${r.changed.length} photo${r.changed.length === 1 ? "" : "s"}${r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : ""}`, r.errors.length > 0);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("busy").hidden = true;
    status("Ready");
  }
}

async function libraryPasteSettings() {
  if (!lib.copied) return toast("First copy the settings of one photo (Ctrl+Shift+C).", true);
  const targets = selectedPhotos().map((p) => p.path).filter((p) => p !== lib.copied);
  if (!targets.length) return toast("Select the photos to paste onto.", true);
  if (!lib.groups) lib.groups = await invoke("paste_groups");
  const dlg = $("paste-dialog");
  $("paste-what").textContent = `From ${fileName(lib.copied)} to ${targets.length} photo${targets.length === 1 ? "" : "s"}. Ticked groups are made the same as the source (settings it doesn't have go back to their defaults); the rest stay as they are.`;
  const list = $("paste-groups");
  list.replaceChildren(...lib.groups.map((g) => {
    const row = Object.assign(document.createElement("label"), { className: "inline-check" });
    const box = Object.assign(document.createElement("input"), { type: "checkbox", value: g.name, checked: lib.pasteChoice?.has(g.name) ?? g.default });
    row.append(box, Object.assign(document.createElement("span"), { textContent: g.label }));
    return row;
  }));
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const groups = [...list.querySelectorAll("input:checked")].map((b) => b.value);
  lib.pasteChoice = new Set(groups);
  if (!groups.length) return;
  status(`Pasting settings onto ${targets.length} photos…`);
  try {
    const r = await invoke("paste_settings", { source: lib.copied, targets, groups });
    for (const path of targets) {
      const p = libPhoto(path);
      if (p) {
        p.edited = true;
        updateCell(p);
        stripUpdate(path, { edited: true });
      }
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    toast(`Pasted onto ${r.pasted} photo${r.pasted === 1 ? "" : "s"}${r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : ""}`, r.errors.length > 0);
    status(`Pasted settings onto ${r.pasted} photos`);
  } catch (e) {
    toast(String(e), true);
  }
}

// The export dialog, for many photos: same choices, one file each.
function libraryExport() {
  const sel = selectedPhotos();
  if (!sel.length) return toast("Select the photos to export.", true);
  const img = lib.cells.get(sel[0].path)?.querySelector("img");
  const aspect = img?.naturalWidth ? img.naturalWidth / img.naturalHeight : 1.5;
  return exportFlow({
    paths: sel.map((p) => p.path),
    aspect,
    run: async (options, after) => {
      const off = await tauri.event.listen("library-progress", (e) => {
        const p = e.payload;
        status(p.done < p.total ? `Exporting ${p.done + 1} of ${p.total}: ${p.name}` : "Exported");
      });
      $("busy").hidden = false;
      try {
        const r = await invoke("export_photos", { paths: sel.map((p) => p.path), options, dir: lastExportDir() });
        if (!r) return;
        if (r.written.length) store.setText("amb.exportDir", dirOf(r.written[0]));
        const failed = r.errors.length ? `; ${r.errors.length} failed: ${r.errors[0]}` : "";
        toast(`Exported ${r.written.length} photo${r.written.length === 1 ? "" : "s"}${failed}`, r.errors.length > 0);
        if (r.written.length) status(`Exported ${r.written.length} photos to ${dirOf(r.written[0])}`);
        await afterExport(r.written, after);
      } finally {
        off();
        $("busy").hidden = true;
      }
    },
  });
}

// Photo Merge > HDR: the selected brackets into one DNG, which then opens.
async function libraryMergeHdr() {
  const sel = selectedPhotos().filter((p) => !p.copy);
  if (sel.length < 2) return toast("Select two or more raws of one scene, shot at different exposures.", true);
  const dlg = $("hdr-dialog");
  $("hdr-what").textContent = `${sel.length} photos: ${sel.map((p) => p.name).join(", ")}`;
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  status(`Merging ${sel.length} photos to HDR…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("merge_hdr", {
      paths: sel.map((p) => p.path),
      align: $("hdr-align").checked,
      deghost: $("hdr-deghost").value,
      autoTone: $("hdr-tone").checked,
    });
    const stops = r.stops.map((s) => `${s >= 0 ? "+" : ""}${s.toFixed(1)}`).join(", ");
    status(`Merged into ${fileName(r.output)} (exposures ${stops} EV)`);
    if (lib.collection) await addToCollection([r.output], lib.collection);
    else await loadLibrary(lib.dir);
    await libraryOpen(r.output);
  } catch (e) {
    toast(String(e), true);
    status("HDR merge failed");
  } finally {
    $("busy").hidden = true;
  }
}

// Photo Merge > Panorama: the selected frames into one DNG, which then opens.
async function libraryMergePano() {
  const sel = selectedPhotos().filter((p) => !p.copy);
  if (sel.length < 2) return toast("Select two or more overlapping raws.", true);
  const dlg = $("pano-dialog");
  $("pano-what").textContent = `${sel.length} photos: ${sel.map((p) => p.name).join(", ")}`;
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  status(`Stitching ${sel.length} photos into a panorama…`);
  $("busy").hidden = false;
  try {
    const r = await invoke("merge_pano", {
      paths: sel.map((p) => p.path),
      projection: $("pano-projection").value,
      autoCrop: $("pano-crop").checked,
      lensCorrections: $("pano-lens").checked,
      autoTone: $("pano-tone").checked,
    });
    const corrected = r.lens_profiles.filter(Boolean).length;
    const lens = $("pano-lens").checked && corrected < r.lens_profiles.length
      ? `; no lens profile for ${r.lens_profiles.length - corrected} of them`
      : "";
    status(`Stitched into ${fileName(r.output)} (${r.projection}, ${Math.round(r.field[0])}° across${lens})`);
    if (lib.collection) await addToCollection([r.output], lib.collection);
    else await loadLibrary(lib.dir);
    await libraryOpen(r.output);
  } catch (e) {
    toast(String(e), true);
    status("Panorama merge failed");
  } finally {
    $("busy").hidden = true;
  }
}

async function libraryCollage() {
  const sel = selectedPhotos();
  if (!sel.length) return;
  await collageFlow();
  setTray(sel.map((p) => ({ ...p })));
  fillEmpty();
}

// SPDX-License-Identifier: GPL-3.0-or-later
// The library's actions on many photos: open, copy and paste an edit, export,
// and start a collage. Split out of library.js.
"use strict";

// ---------------------------------------------------------------- open, copy and paste, export, collage

async function libraryOpen(path) {
  if (!path) return;
  if (libPhoto(path)?.offline) return toast(t("This photo's drive isn't plugged in. Plug it in to open the photo."), true);
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
    return toast(t("Copied the settings of {name}: select photos in the filmstrip (Ctrl- or Shift-click), then Paste settings", { name: fileName(ui.info.path) }));
  }
  const sel = selectedPhotos();
  if (sel.length !== 1) return toast(t("Select the one photo to copy the settings from."), true);
  lib.copied = sel[0].path;
  toast(t("Copied the settings of {name}", { name: sel[0].name }));
  renderPanel();
}

// A virtual copy of one photo: a second edit, kept as its own sidecar.
async function makeVirtualCopy(path) {
  try {
    const copy = await invoke("create_virtual_copy", { path });
    status(t("Made a virtual copy of {name}", { name: fileName(path) }));
    return copy;
  } catch (e) {
    showError(e);
    return null;
  }
}

// Ctrl+': in the library, of the selected photo; in the editor, of the open
// photo, which then opens.
async function virtualCopyKey() {
  if (lib.on) {
    const sel = libraryTargets();
    if (sel.length !== 1) return toast(t("Select the one photo to make a virtual copy of."), true);
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
  const ask = copies.length === 1
    ? t("Delete {name}? Only the copy's edit goes; the photo stays.", { name: copies[0].name })
    : t("Delete {n} virtual copies? Only the copy's edit goes; the photo stays.", { n: copies.length });
  if (!await askYes(ask)) return;
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
      showError(e);
    }
  }
  status(t("Deleted {copies}", { copies: plural(copies.length - failed, "virtual copy", "virtual copies") }));
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
  if (!paths.length) return toast(t("Select the photos for the sheet."), true);
  const dlg = $("contact-dialog");
  $("contact-what").textContent = t("{photos}, in the order shown.", { photos: plural(paths.length, "photo") });
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
  status(t("Making a contact sheet of {n} photos…", { n: paths.length }));
  $("busy").hidden = false;
  try {
    const pages = await invoke("contact_sheet", { paths, options, dir: lastExportDir() });
    if (pages?.length) {
      rememberExportDir(pages[0]);
      toast(pages.length === 1 ? t("Saved the contact sheet: {path}", { path: pages[0] }) : t("Saved {n} pages of contact sheets in {dir}", { n: pages.length, dir: dirOf(pages[0]) }));
    }
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
    status(t("Ready"));
  }
}

// Smart search over the photos the library has open: by the search box's
// words, or (`like`) for photos like one (src/commands/models.rs `smart_search`).
async function smartSearch(like = null) {
  const query = $("lib-search").value.trim();
  if (!like && !query) return toast(t("Type what to look for, then press Smart (a dog on a beach, snowy mountains)."), true);
  const paths = lib.photos.map((p) => p.path);
  status(like ? t("Looking for photos like it…") : t("Looking for “{query}”…", { query }));
  $("busy").hidden = false;
  try {
    const r = await invoke("smart_search", { paths, query: like ? null : query, like });
    lib.found = {
      of: lib.photos,
      rank: new Map(r.found.map(([p], i) => [p, i])),
      label: like ? t("Like {name}", { name: fileName(like) }) : `“${query}”`,
    };
    const chip = $("lib-smart-chip");
    chip.textContent = t("{label}: {n} found ×", { label: lib.found.label, n: r.found.length });
    chip.hidden = false;
    renderLibrary();
    status(r.indexed
      ? t("{n} found with {model} ({indexed} photos indexed first)", { n: r.found.length, model: r.model, indexed: r.indexed })
      : t("{n} found with {model}", { n: r.found.length, model: r.model }));
  } catch (e) {
    showError(e);
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
  if (paths.length < 2) return toast(t("Select the frames of the roll."), true);
  const dlg = $("roll-dialog");
  const focus = paths.includes(lib.focus) ? lib.focus : null;
  $("roll-what").textContent = t("{n} frames. Each is analysed; the roll then shares one film base and levels, with Auto off, so a frame exposed a stop more prints a stop brighter.", { n: paths.length });
  $("roll-baseline").disabled = !focus;
  $("roll-baseline").checked = false;
  $("roll-baseline-label").textContent = focus ? t("Use {name} as the baseline", { name: fileName(focus) }) : t("Use this frame as the baseline (click one first)");
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const options = {
    scenes: $("roll-scenes").checked,
    odd: $("roll-odd").checked,
    own_midtone: $("roll-own-midtone").checked,
    baseline: $("roll-baseline").checked && focus ? paths.indexOf(focus) : null,
  };
  status(t("Analysing {n} negatives…", { n: paths.length }));
  $("busy").hidden = false;
  try {
    const r = await invoke("negative_sync", { paths, options });
    for (const path of r.changed) {
      photoChanged(path, { edited: true });
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    const parts = [];
    if (r.scenes > 1) parts.push(t("{n} scenes", { n: r.scenes }));
    if (r.odd > 0) parts.push(t("{frames} kept their own", { frames: plural(r.odd, "odd frame") }));
    const negatives = plural(r.changed.length, "negative");
    const synced = parts.length ? t("Synced {negatives} ({parts})", { negatives, parts: parts.join(", ") }) : t("Synced {negatives}", { negatives });
    toast(r.errors.length ? t("{text}; {n} failed: {error}", { text: synced, n: r.errors.length, error: r.errors[0] }) : synced, r.errors.length > 0);
    status(t("One conversion for {n} frames", { n: r.changed.length }));
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
  }
}

// The "match to this photo" row in the convert dialog: made once, pointed at each run's choices.
const convertLike = { row: null, pick: null, info: null, current: null };

// Raws to JPEGs (src/commands/batch.rs `convert_raws`): in the library
// the selection, or every photo shown when none is selected; in the editor
// the open photo's folder (the filmstrip), like the open photo.
async function convertRawsFlow() {
  let paths, what, like;
  if (lib.on) {
    const sel = selectedPhotos();
    paths = (sel.length ? sel : lib.shown).map((p) => p.path);
    what = sel.length ? "selected" : "shown";
    like = lib.focus && paths.includes(lib.focus) ? lib.focus : null;
  } else {
    if (!ui.info) return toast(t("Open a photo first."), true);
    // Its edit as it is now, for "like this photo" and for itself.
    await invoke("save_sidecar").catch(() => {});
    const picked = stripSelected();
    paths = (picked.length ? picked : strip.photos?.length ? strip.photos : [{ path: ui.info.path }]).map((p) => p.path);
    what = picked.length ? "strip" : "folder";
    like = ui.info.path;
  }
  if (!paths.length) return;
  const dlg = $("convert-dialog");
  const f = dlg.querySelector("form");
  const photos = plural(paths.length, "photo");
  $("convert-what").textContent = what === "selected" ? t("{photos} selected.", { photos })
    : what === "shown" ? t("{photos} shown.", { photos })
      : what === "strip" ? t("{photos} selected in the filmstrip.", { photos })
        : t("{photos} in this folder.", { photos });
  // Which photo to match: the edited ones among them (the focused or open
  // one first), to pick from.
  const info = (path) => (lib.on ? libPhoto(path) : strip.photos?.find((p) => p.path === path)) ?? { path };
  const edited = paths.filter((p) => p !== like && info(p).edited);
  const choices = [...(like ? [like] : []), ...edited].slice(0, 30);
  if (!like && choices.length) like = choices[0];
  const showLike = () => {
    $("convert-like").disabled = !like;
    $("convert-like-label").replaceChildren(...(like
      ? [el("b", {}, t("Like {name}", { name: fileName(like) })), choices.length > 1 ? t(": every photo matched to it (pick another below)") : t(": every photo matched to it")]
      : [el("b", {}, t("Like one I edited")), t(": edit one of them first")]));
    convertLike.row?.mark();
  };
  convertLike.pick = (path) => {
    like = path;
    f.convert_look.value = "like";
    showLike();
  };
  convertLike.row ??= photoRow($("convert-like-pick"), {
    cell: ({ path }) => el("button", { type: "button", title: t("Match every photo to {name}", { name: fileName(path) }) }, el("img", { alt: fileName(path) })),
    thumb: ({ path }, b) => bigImage(convertLike.info(path), 128, true).then((u) => { b.firstChild.src = u; }).catch(() => {}),
    open: ({ path }) => convertLike.pick(path),
    isCurrent: ({ path }) => path === convertLike.current(),
  });
  convertLike.info = info;
  convertLike.current = () => like;
  convertLike.row.set((choices.length > 1 ? choices : []).map((path) => ({ path })));
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
  status(t("Converting {n} raws…", { n: paths.length }));
  $("busy").hidden = false;
  try {
    const r = await invoke("convert_raws", { paths, options });
    if (!r) return;
    for (const path of paths) refreshThumb(path);
    const parts = [t("{jpegs} written", { jpegs: plural(r.written.length, "JPEG") })];
    if (r.camera_jpegs) parts.push(r.camera_jpegs === 1 ? t("{n} as the camera saved it", { n: r.camera_jpegs }) : t("{n} as the camera saved them", { n: r.camera_jpegs }));
    if (r.skipped) parts.push(t("{n} already there", { n: r.skipped }));
    if (r.left_out) parts.push(t("{n} left out (not the best of their burst)", { n: r.left_out }));
    if (r.errors.length) parts.push(t("{n} failed: {error}", { n: r.errors.length, error: r.errors[0] }));
    toast(parts.join("; "), r.errors.length > 0);
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
    status(t("Ready"));
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
  $("preset-pick-title").textContent = paths.length > 1 ? t("Apply a preset to {n} photos", { n: paths.length }) : t("Apply a preset to {name}", { name: fileName(paths[0]) });
  amount.oninput = () => { $("preset-pick-amount-out").value = amount.value; };
  amount.oninput();
  let chosen = null;
  const render = () => {
    const q = filter.value.trim().toLowerCase();
    const shown = presetsShown().filter((p) => !q || `${p.group} ${p.name}`.toLowerCase().includes(q));
    $("preset-pick-list").replaceChildren(...shown.map((p) => {
      const b = el("button", { type: "button", class: "small", title: p.group, translate: "no" }, p.name);
      b.addEventListener("click", () => { chosen = p; dlg.close("ok"); });
      return b;
    }));
    if (!shown.length) $("preset-pick-list").append(el("p", { class: "hint" }, t("No presets match.")));
  };
  filter.value = "";
  filter.oninput = render;
  render();
  dlg.showModal();
  filter.focus();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok" || !chosen) return;
  status(t("Applying \"{name}\" to {n} photos…", { name: chosen.name, n: paths.length }));
  $("busy").hidden = false;
  try {
    const r = await invoke("apply_to_photos", {
      paths,
      label: `preset ${chosen.name}`,
      commands: await presetCommands(chosen.path, { amount: Number(amount.value) }),
    });
    for (const path of r.changed) {
      photoChanged(path, { edited: true });
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    const applied = t("\"{name}\" applied to {photos}", { name: chosen.name, photos: plural(r.changed.length, "photo") });
    toast(r.errors.length ? t("{text}; {n} failed: {error}", { text: applied, n: r.errors.length, error: r.errors[0] }) : applied, r.errors.length > 0);
  } catch (e) {
    showError(e);
  } finally {
    $("busy").hidden = true;
    status(t("Ready"));
  }
}

async function libraryPasteSettings() {
  if (!lib.copied) return toast(t("First copy the settings of one photo (Ctrl+Shift+C)."), true);
  const targets = selectedPhotos().map((p) => p.path).filter((p) => p !== lib.copied);
  if (!targets.length) return toast(t("Select the photos to paste onto."), true);
  if (!lib.groups) lib.groups = await invoke("paste_groups");
  const dlg = $("paste-dialog");
  $("paste-what").textContent = t("From {name} to {photos}. Ticked groups are made the same as the source (settings it doesn't have go back to their defaults); the rest stay as they are.", { name: fileName(lib.copied), photos: plural(targets.length, "photo") });
  const list = $("paste-groups");
  list.replaceChildren(...lib.groups.map((g) => {
    const row = el("label", { class: "inline-check" });
    const box = el("input", { type: "checkbox", value: g.name, checked: lib.pasteChoice?.has(g.name) ?? g.default });
    row.append(box, el("span", {}, t(g.label)));
    return row;
  }));
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  const groups = [...list.querySelectorAll("input:checked")].map((b) => b.value);
  lib.pasteChoice = new Set(groups);
  if (!groups.length) return;
  status(t("Pasting settings onto {n} photos…", { n: targets.length }));
  try {
    const r = await invoke("paste_settings", { source: lib.copied, targets, groups });
    for (const path of targets) {
      photoChanged(path, { edited: true });
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    const pasted = t("Pasted onto {photos}", { photos: plural(r.pasted, "photo") });
    toast(r.errors.length ? t("{text}; {n} failed: {error}", { text: pasted, n: r.errors.length, error: r.errors[0] }) : pasted, r.errors.length > 0);
    status(t("Pasted settings onto {n} photos", { n: r.pasted }));
  } catch (e) {
    showError(e);
  }
}

// The export dialog, for many photos: same choices, one file each.
function libraryExport() {
  const sel = selectedPhotos();
  if (!sel.length) return toast(t("Select the photos to export."), true);
  const img = lib.cells.get(sel[0].path)?.querySelector("img");
  const aspect = img?.naturalWidth ? img.naturalWidth / img.naturalHeight : 1.5;
  return exportFlow({
    paths: sel.map((p) => p.path),
    aspect,
    run: async (options, after) => {
      const off = await tauri.event.listen("library-progress", (e) => {
        const p = e.payload;
        status(p.done < p.total ? t("Exporting {n} of {total}: {name}", { n: p.done + 1, total: p.total, name: p.name }) : t("Exported"));
      });
      $("busy").hidden = false;
      try {
        const r = await invoke("export_photos", { paths: sel.map((p) => p.path), options, dir: lastExportDir() });
        if (!r) return;
        if (r.written.length) store.setText("amb.exportDir", dirOf(r.written[0]));
        const exported = t("Exported {photos}", { photos: plural(r.written.length, "photo") });
        toast(r.errors.length ? t("{text}; {n} failed: {error}", { text: exported, n: r.errors.length, error: r.errors[0] }) : exported, r.errors.length > 0);
        if (r.written.length) status(t("Exported {n} photos to {dir}", { n: r.written.length, dir: dirOf(r.written[0]) }));
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
  if (sel.length < 2) return toast(t("Select two or more raws of one scene, shot at different exposures."), true);
  const dlg = $("hdr-dialog");
  $("hdr-what").textContent = t("{n} photos: {names}", { n: sel.length, names: sel.map((p) => p.name).join(", ") });
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  status(t("Merging {n} photos to HDR…", { n: sel.length }));
  $("busy").hidden = false;
  try {
    const r = await invoke("merge_hdr", {
      paths: sel.map((p) => p.path),
      align: $("hdr-align").checked,
      deghost: $("hdr-deghost").value,
      autoTone: $("hdr-tone").checked,
    });
    const stops = r.stops.map((s) => `${s >= 0 ? "+" : ""}${s.toFixed(1)}`).join(", ");
    status(t("Merged into {name} (exposures {stops} EV)", { name: fileName(r.output), stops }));
    if (lib.collection) await addToCollection([r.output], lib.collection);
    else await loadLibrary(lib.dir);
    await libraryOpen(r.output);
  } catch (e) {
    showError(e);
    status(t("HDR merge failed"));
  } finally {
    $("busy").hidden = true;
  }
}

// Photo Merge > Panorama: the selected frames into one DNG, which then opens.
async function libraryMergePano() {
  const sel = selectedPhotos().filter((p) => !p.copy);
  if (sel.length < 2) return toast(t("Select two or more overlapping raws."), true);
  const dlg = $("pano-dialog");
  $("pano-what").textContent = t("{n} photos: {names}", { n: sel.length, names: sel.map((p) => p.name).join(", ") });
  dlg.showModal();
  const choice = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  if (choice !== "ok") return;
  status(t("Stitching {n} photos into a panorama…", { n: sel.length }));
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
    const vars = { name: fileName(r.output), projection: r.projection, degrees: Math.round(r.field[0]), n: r.lens_profiles.length - corrected };
    status($("pano-lens").checked && corrected < r.lens_profiles.length
      ? t("Stitched into {name} ({projection}, {degrees}° across; no lens profile for {n} of them)", vars)
      : t("Stitched into {name} ({projection}, {degrees}° across)", vars));
    if (lib.collection) await addToCollection([r.output], lib.collection);
    else await loadLibrary(lib.dir);
    await libraryOpen(r.output);
  } catch (e) {
    showError(e);
    status(t("Panorama merge failed"));
  } finally {
    $("busy").hidden = true;
  }
}

// A collage of the selected photos, or (`all`) of every photo shown, which
// the collage's Batch then lays out as pages of one layout.
async function libraryCollage(all = false) {
  const photos = all ? lib.shown : selectedPhotos();
  if (!photos.length) return toast(all ? t("No photos shown in the library.") : t("Select the photos first."), true);
  await collageFlow();
  setTray(photos.map((p) => ({ ...p })));
  fillEmpty();
  if (all) {
    $("col-batch").scrollIntoView({ block: "center" });
    status(t("{photos} in the tray: Batch → Export pages… makes them pages of this layout", { photos: plural(photos.length, "photo") }));
  }
}

// SPDX-License-Identifier: GPL-3.0-or-later
// The editor's main path: open, edit, undo, redo, a panel's tabs, help, export.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/smoke.js photo.dng
// (src/uitest.rs; the helpers are in ui/uitest.js).

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

const before = t.setting("Exposure2012") ?? 0;
await t.step("a slider edits the photo, as one history step", async () => {
  const steps = t.historyLength();
  const src = $("preview").src;
  t.setSlider("Exposure2012", 1);
  await t.until("Exposure +1", () => t.setting("Exposure2012") === 1);
  await t.until("a history step", () => t.historyLength() === steps + 1);
  await t.until("a new preview", () => $("preview").src !== src);
  t.expect(t.row("Exposure2012").querySelector(".value").value === "+1.00", "the box to show +1.00");
});

await t.step("Ctrl+Z undoes and Ctrl+Shift+Z redoes", async () => {
  t.key("Ctrl+Z");
  await t.until("Exposure back", () => (t.setting("Exposure2012") ?? 0) === before);
  t.key("Ctrl+Shift+Z");
  await t.until("Exposure +1 again", () => t.setting("Exposure2012") === 1);
});

await t.step("every panel opens", async () => {
  for (const d of document.querySelectorAll("details.panel")) {
    if (!d.open) t.click(d.querySelector("summary"));
    await t.sleep(30);
  }
  t.expect(t.visible(t.row("HueAdjustmentRed")), "the Color Mixer open");
});

await t.step("the right column's tabs show one group of panels at a time", async () => {
  const tabs = [...document.querySelectorAll("#col-right .column-tab")];
  t.expect(tabs.length >= 2, `tabs: ${tabs.map((b) => b.textContent).join(", ")}`);
  const shown = () => [...document.querySelectorAll("#col-right > details.panel")].filter((d) => !d.classList.contains("tab-off")).map((d) => d.dataset.tab);
  t.click(tabs[1]);
  t.expect(shown().length && shown().every((n) => n === tabs[1].dataset.tab), "only the second tab's panels");
  // Search finds a setting in another tab and brings that tab forward.
  revealSetting("Exposure2012");
  t.expect(t.visible(t.row("Exposure2012")), "Exposure shown again");
});

await t.step("no status bar; scrolling on the photo can step through the folder", async () => {
  t.expect(document.body.classList.contains("status-off") && document.querySelector(".statusbar").getBoundingClientRect().height === 0, "the status bar hidden");
  status("A message");
  t.expect(!$("status-note").hidden, "a message shows over the photo");
  const real = stepPhoto;
  let stepped = 0;
  stepPhoto = (d) => (stepped += d);
  const wheelAt = () => $("viewer").dispatchEvent(new WheelEvent("wheel", { deltaY: 100, bubbles: true, cancelable: true }));
  wheelAt();
  t.expect(stepped === 0, "off by default");
  $("settings-wheel-photos").checked = true;
  $("settings-wheel-photos").dispatchEvent(new Event("change"));
  wheelAt();
  stepPhoto = real;
  $("settings-wheel-photos").checked = false;
  $("settings-wheel-photos").dispatchEvent(new Event("change"));
  t.expect(stepped === 1, "on, scrolling down goes to the next photo");
});

await t.step("a section's buttons never cover the row under them, even in a narrow column", async () => {
  const col = $("col-right");
  const was = col.style.width;
  col.style.width = "260px";
  for (const d of col.querySelectorAll("details.panel")) d.open = true;
  await new Promise((r) => requestAnimationFrame(r));
  const bad = [];
  for (const label of col.querySelectorAll(".section-label")) {
    const next = label.nextElementSibling;
    if (!next || !label.querySelector(".section-action")) continue;
    const bottom = Math.max(...[...label.querySelectorAll(".section-action")].map((b) => b.getBoundingClientRect().bottom));
    if (bottom > next.getBoundingClientRect().top + 1) bad.push(label.querySelector(".section-name").textContent);
  }
  col.style.width = was;
  t.expect(!bad.length, `buttons over the next row in: ${bad.join(", ")}`);
});

await t.step("the Color Mixer shows one tab at a time", async () => {
  t.expect(!t.visible(t.row("SaturationAdjustmentRed")), "Saturation rows hidden on the Hue tab");
  const tab = [...document.querySelectorAll(".tabs button")].find((b) => b.textContent === "Saturation");
  t.click(tab);
  t.expect(t.visible(t.row("SaturationAdjustmentRed")), "Saturation rows shown");
  t.expect(!t.visible(t.row("HueAdjustmentRed")), "Hue rows hidden");
});

await t.step("a ? opens its help card", async () => {
  t.click(t.row("Exposure2012").querySelector(".help-mark"));
  await t.until("the card", () => help.card && t.visible(help.card));
  t.expect(help.card.textContent.includes("Exposure"), "the card to name Exposure");
  t.key("Escape");
});

await t.step("Settings says how the monitor's colours are handled", async () => {
  const s = await t.invoke("set_monitor_profile", { choice: { mode: "off" } });
  t.expect(s.choice.mode === "off" && $("settings-monitor-note").textContent !== "", "a monitor note");
  await t.invoke("set_monitor_profile", { choice: { mode: "auto" } });
  t.log(s.note);
});

await t.step("applying a preset shows no note, and its button no tooltip", async () => {
  const p = ui.presets[0];
  const b = [...document.querySelectorAll("#preset-list .preset")].find((x) => x.textContent === p.name);
  t.expect(b && !b.title, "no tooltip on the preset");
  $("toast").hidden = true;
  await applyPreset(p);
  await t.sleep(300);
  t.expect($("toast").hidden, `no note: ${$("toast").textContent}`);
  t.key("Ctrl+Z");
});

await t.step("a preset's notes for scripts become one short line", async () => {
  const said = notesForPeople([
    "copied 23 setting(s) unknown to the registry without validation: A, B, C",
    "copied Temperature, Tint unscaled at 50%: no current value",
    "WhiteBalance set to \"Custom\" so that Temperature takes effect",
  ]);
  t.expect(said === "", `nothing to say: ${said}`);
  t.expect(notesForPeople(["no change"]) === "No change", "a no-op says so");
});

await t.step("the filmstrip's stars rate the open photo", async () => {
  const star = document.querySelector('#strip-rating [data-stars="3"]');
  t.click(star);
  await t.until("rated", () => infoState.info.rating === 3, 10000);
  t.expect(star.classList.contains("on") && !document.querySelector('#strip-rating [data-stars="4"]').classList.contains("on"), "three stars lit");
  t.click(star);
  await t.until("cleared", () => !infoState.info.rating, 10000);
});

await t.step("a filmstrip photo's stars rate it", async () => {
  const stars = strip.cells.get(ui.info.path).querySelector(".film-stars");
  t.expect(stars.classList.contains("editable") && stars.children.length === 5, "five stars to click");
  stars.querySelector('[data-stars="2"]').click();
  await t.until("rated", () => infoState.info.rating === 2, 10000);
  t.expect(stars.querySelectorAll(".on").length === 2, "two lit");
  stars.querySelector('[data-stars="2"]').click();
  await t.until("cleared", () => !infoState.info.rating, 10000);
});

await t.step("the whole photo fills the viewer", async () => {
  const img = $("preview").getBoundingClientRect();
  const view = $("viewer").getBoundingClientRect();
  t.expect(Math.abs(img.width - view.width) <= 2 || Math.abs(img.height - view.height) <= 2, `photo ${Math.round(img.width)}×${Math.round(img.height)} in ${Math.round(view.width)}×${Math.round(view.height)}`);
});

await t.step("zooming in shows the photo at once; a drag never shows black", async () => {
  setZoom(true, 0.3, 0.3);
  t.expect(!$("zoom-under").hidden && $("viewer").classList.contains("actual"), "the enlarged photo shows before the 100% crop arrives");
  await t.until("the 100% view", () => ui.zoom.meta, 60000);
  t.expect($("preview").style.visibility === "", "the sharp crop on top");
  placeRegion(-400, -300);
  const under = $("zoom-under").getBoundingClientRect();
  const view = $("viewer").getBoundingClientRect();
  t.expect(under.left <= view.left + 1 && under.top <= view.top + 1 && under.right >= view.right - 1 && under.bottom >= view.bottom - 1, "photo under the whole view while dragging");
  placeRegion(0, 0);
});

await t.step("the crop tool opened while zoomed in starts from the view", async () => {
  t.key("r");
  await t.until("the crop tool", () => ui.crop.on);
  const r = ui.crop.tool.rect();
  t.expect(r.width < ui.info.width * 0.95 || r.height < ui.info.height * 0.95, `a crop smaller than the photo: ${Math.round(r.width)}×${Math.round(r.height)}`);
  t.key("Escape");
  await t.until("the crop tool closed", () => !ui.crop.on);
  t.expect(!ui.zoom.on && $("zoom-under").hidden, "back to the whole photo");
});

await t.step("dragging the filmstrip moves along it and doesn't open a photo", async () => {
  const list = $("filmstrip-list");
  const spacer = Object.assign(document.createElement("div"), { style: "flex: none; width: 3000px" });
  list.append(spacer);
  list.scrollLeft = 0;
  const cell = list.querySelector(".film-cell") ?? list.firstElementChild;
  const at = cell.getBoundingClientRect();
  let opened = false;
  const real = openPath;
  openPath = async () => (opened = true);
  const ev = (type, x) => cell.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: at.top + 10, pointerId: 7, button: 0, pointerType: "mouse" }));
  ev("pointerdown", at.left + 40);
  ev("pointermove", at.left + 20);
  ev("pointermove", at.left - 160);
  ev("pointerup", at.left - 160);
  cell.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  openPath = real;
  spacer.remove();
  t.expect(list.scrollLeft > 100 || list.scrollWidth <= list.clientWidth, `moved along: ${list.scrollLeft}`);
  t.expect(!opened, "no photo opened");
});

await t.step("turning in the crop tool snaps to the photo's straight lines", async () => {
  t.key("r");
  await t.until("the crop tool", () => ui.crop.on);
  const lines = await t.invoke("guide_lines");
  t.expect(lines.length > 0, "the colour chart has straight lines");
  ui.crop.tool.setGuides(lines);
  ui.crop.tool.setDrawMode(false);
  const c = $("crop-canvas");
  const r = c.getBoundingClientRect();
  const [cx, cy, R] = [r.left + r.width / 2, r.top + r.height / 2, r.width / 2 - 3];
  const at = (deg, alt = false) => ({ bubbles: true, clientX: cx + R * Math.cos((deg * Math.PI) / 180), clientY: cy + R * Math.sin((deg * Math.PI) / 180), pointerId: 4, button: 0, altKey: alt });
  c.dispatchEvent(new PointerEvent("pointerdown", at(0)));
  c.dispatchEvent(new PointerEvent("pointermove", at(0.3)));
  t.expect(Math.abs(ui.crop.tool.rect().angle) < 0.05, `a small turn snaps back level: ${ui.crop.tool.rect().angle}`);
  c.dispatchEvent(new PointerEvent("pointermove", at(0.3, true)));
  t.expect(Math.abs(ui.crop.tool.rect().angle) > 0.2, `Alt turns freely: ${ui.crop.tool.rect().angle}`);
  c.dispatchEvent(new PointerEvent("pointerup", at(0.3, true)));
  t.key("Escape");
  await t.until("the crop tool closed", () => !ui.crop.on);
});

await t.step("Level in the crop tool keeps a level photo level", async () => {
  t.key("r");
  await t.until("the crop tool", () => ui.crop.on);
  t.click("#crop-level");
  await t.until("Level done", () => !$("crop-level").disabled && $("crop-angle-num").value !== "", 30000);
  t.expect(Math.abs(Number($("crop-angle").value)) < 0.5, `the colour chart is level already: ${$("crop-angle").value}`);
  t.key("Escape");
  await t.until("the crop tool closed", () => !ui.crop.on);
});

await t.step("a click near the photo's corner zooms in without showing past its edge", async () => {
  setZoom(true, 0.001, 0.999);
  await t.until("the 100% view", () => ui.zoom.meta, 60000);
  const img = $("preview").getBoundingClientRect();
  const view = $("viewer").getBoundingClientRect();
  t.expect(img.left <= view.left + 1 && img.bottom >= view.bottom - 1, `no void: photo ${Math.round(img.left)},${Math.round(img.bottom)} viewer ${Math.round(view.left)},${Math.round(view.bottom)}`);
  t.expect(ui.zoom.cx > 0.001 && ui.zoom.cy < 0.999, "the centre moved in from the corner");
  setZoom(false);
  await t.until("the whole photo", () => !$("viewer").classList.contains("actual"));
});

await t.step("Minimalism hides the chrome; the panels slide in from the edge", async () => {
  const before = { layout: ui.looks.active_layout, theme: ui.looks.active_theme };
  Object.assign(ui.looks, { active_layout: "builtin:minimalism", active_theme: "builtin:minimalism" });
  applyActiveLooks();
  const body = document.body.classList;
  t.expect(body.contains("chrome-hidden") && body.contains("topbar-off"), "the chrome hidden");
  t.expect(!$("edge-right").hidden && $("filmstrip").hidden, "a right edge strip, no filmstrip");
  t.key("Ctrl+]");
  t.expect(body.contains("peek-right"), "Ctrl+] slides the right column in");
  t.key("Ctrl+]");
  t.expect(!body.contains("peek-right"), "and out");
  // The pointer leaving it hides it again, even with a panel open.
  peekColumn("right", true);
  t.expect($("col-right").querySelector("details[open]"), "an open panel");
  $("col-right").dispatchEvent(new MouseEvent("mouseleave"));
  await t.until("the column hides after the pointer leaves", () => !body.contains("peek-right"), 3000);
  Object.assign(ui.looks, { active_layout: before.layout, active_theme: before.theme });
  applyActiveLooks();
  t.expect(!body.contains("chrome-hidden") && $("edge-right").hidden, "the chrome back");
});

await t.step("a group's 2 × 2 is a square of its best four", async () => {
  await collageFlow();
  await t.until("the collage maker", () => $("collage-dialog").open && collage.templates.length);
  const p = ui.info.path;
  collage.tray = [{ path: p, name: "a" }, { path: `${p}#2`, name: "b" }];
  collage.groups = [[p, `${p}#2`]];
  renderTray();
  t.expect(document.querySelector(".tray-group button")?.textContent === "2 × 2", "a 2 × 2 button on the group");
  t.expect($("col-group").textContent === "Ungroup" && !$("col-batch-groups-row").hidden, "Ungroup, and pages per group");
  groupGrid([p, p, p, p, p]);
  t.expect(collage.t.cells.length === 4 && collage.t.width === collage.t.height, "a square 2 × 2");
  t.expect(collage.cells.length === 4 && collage.cells.every((c) => c.path === p), "four photos in it");
  t.click("#col-no-borders");
  t.expect(collage.t.gap === 0 && collage.t.margin === 0 && $("col-gap-out").textContent === "0%", "no borders");
  $("col-batch-groups").checked = true;
  updateBatchNote();
  t.expect($("col-batch-note").textContent.includes("fewer than 4"), $("col-batch-note").textContent);
  setTray([]);
  closeCollage();
});

await t.step("Save to… keeps a preset in a folder of yours; a preset can go to the hub", async () => {
  t.setSlider("Exposure2012", 0.5);
  const flow = savePresetFlow("Shared look");
  await t.until("the Save dialog", () => $("save-preset-dialog").open);
  $("save-preset-dialog").close("to");
  await flow;
  const p = ui.presets.find((x) => x.name === "Shared look");
  t.expect(p && p.path.includes("ambrotype-ui-test"), `saved in the folder chosen: ${p?.path}`);
  // Its right-click menu offers the hub and the file.
  const row = [...document.querySelectorAll("#preset-list .preset")].find((b) => b.textContent === "Shared look");
  row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
  const items = [...document.querySelectorAll(".ctx-menu button")].map((b) => b.textContent);
  t.expect(items.includes("Share to the hub…") && items.includes("Show the file"), items.join(", "));
  document.querySelector(".ctx-menu")?.remove();
  t.key("Ctrl+Z");
});

await t.step("right-click on the photo: Copy image puts it on the clipboard", async () => {
  photoMenu(new MouseEvent("contextmenu", { clientX: 400, clientY: 300 }));
  const items = [...document.querySelectorAll(".ctx-menu button")].map((b) => b.textContent);
  t.expect(["Copy image", "Copy file path", "Show in folder", "Export…"].every((i) => items.includes(i)), items.join(", "));
  document.querySelector(".ctx-menu")?.remove();
  const [w, h] = await t.invoke("copy_image", { path: ui.info.path });
  t.expect(w > 0 && h > 0 && Math.max(w, h) <= 3072, `copied ${w}×${h}`);
});

await t.step("Pick base: a click on the photo stores the film base; Alt-click forgets it", async () => {
  const pick = document.querySelector(".pick-base");
  t.expect(pick, "the Film Negative panel's Pick base button");
  const steps = t.historyLength();
  t.click(pick);
  await t.until("picking the base", () => ui.picking === "base" && $("viewer").classList.contains("picking"));
  const r = $("preview").getBoundingClientRect();
  const at = { clientX: r.left + r.width * 0.5, clientY: r.top + r.height * 0.5, button: 0, bubbles: true };
  $("preview").dispatchEvent(new MouseEvent("mousedown", at));
  window.dispatchEvent(new MouseEvent("mouseup", at));
  await t.until("a film base", () => t.setting("amb:FilmBaseRed") != null, 30000);
  t.expect(t.setting("amb:NegativeConvert") === true, "the conversion on");
  t.expect(!ui.picking && !pick.classList.contains("active"), "picking over");
  t.expect(t.historyLength() === steps + 1, "one history step");
  pick.dispatchEvent(new MouseEvent("click", { altKey: true, bubbles: true }));
  await t.until("the base measured again", () => t.setting("amb:FilmBaseRed") == null);
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
  await t.until("the photo as it was", () => !t.setting("amb:NegativeConvert"));
});

await t.step("Ctrl+E exports a JPEG", async () => {
  t.key("Ctrl+E");
  await t.until("the Export dialog", () => $("export-dialog").open);
  t.click("#export-go");
  await t.until("the export", () => t.status().startsWith("Exported "), 120000);
  t.log(t.status());
  t.expect(lastExportDir() && t.status().includes(lastExportDir()), `the folder kept for next time: ${lastExportDir()}`);
});

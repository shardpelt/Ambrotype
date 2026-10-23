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

await t.step("a preset's notes for scripts become one short line", async () => {
  const said = notesForPeople([
    "copied 23 setting(s) unknown to the registry without validation: A, B, C",
    "copied Temperature, Tint unscaled at 50%: no current value",
    "WhiteBalance set to \"Custom\" so that Temperature takes effect",
  ]);
  t.expect(said === "23 of its settings aren't supported yet (kept in the file for Lightroom)", said);
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

await t.step("Ctrl+E exports a JPEG", async () => {
  t.key("Ctrl+E");
  await t.until("the Export dialog", () => $("export-dialog").open);
  t.click("#export-go");
  await t.until("the export", () => t.status().startsWith("Exported "), 120000);
  t.log(t.status());
});

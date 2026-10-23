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

await t.step("Ctrl+E exports a JPEG", async () => {
  t.key("Ctrl+E");
  await t.until("the Export dialog", () => $("export-dialog").open);
  t.click("#export-go");
  await t.until("the export", () => t.status().startsWith("Exported "), 120000);
  t.log(t.status());
});

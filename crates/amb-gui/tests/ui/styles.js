// SPDX-License-Identifier: GPL-3.0-or-later
// Styles: a look saved from a photo, applied to a photo as its own moves.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/styles.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

const start = () => t.setting("Exposure2012") ?? 0;
let base = 0;

await t.step("a style is saved from the photo's brighter edit", async () => {
  base = start();
  t.setSlider("Exposure2012", base + 1);
  await t.until("Exposure raised", () => start() > base + 0.9);
  // The style measures the reference's saved edit.
  await invoke("save_sidecar");
  const made = await invoke("style_save", { name: "Bright test", references: [ui.info.path], focus: "tone" });
  t.expect(made.name === "Bright test" && made.focus === "tone", "the saved style");
  t.key("Ctrl+Z");
  await t.until("Exposure back", () => Math.abs(start() - base) < 0.05);
});

await t.step("the Styles dialog lists it", async () => {
  KEY_ACTIONS.find((a) => a.id === "styles").run();
  await t.until("the dialog", () => t.visible("#styles-dialog") && $("styles-list").textContent.includes("Bright test"));
  t.expect($("styles-empty").hidden, "no empty-list text");
  t.expect(!$("styles-apply-one").disabled, "Apply to this photo is on");
  t.expect($("styles-apply-many").disabled, "no selection, no batch");
});

await t.step("Apply to this photo finds its own Exposure, as one undo step", async () => {
  const steps = t.historyLength();
  t.click($("styles-apply-one"));
  await t.until("Exposure raised by the style", () => start() > base + 0.5, 120000);
  t.expect(t.historyLength() === steps + 1, "one history step");
  t.click($("styles-close"));
  await t.until("the dialog closed", () => !t.visible("#styles-dialog"));
  t.key("Ctrl+Z");
  await t.until("Exposure back", () => Math.abs(start() - base) < 0.05);
});

await t.step("Delete removes the style", async () => {
  const real = askYes;
  askYes = async () => true;
  try {
    KEY_ACTIONS.find((a) => a.id === "styles").run();
    await t.until("the dialog", () => t.visible("#styles-dialog"));
    t.click($("styles-delete"));
    await t.until("the style gone", () => !$("styles-list").textContent.includes("Bright test"));
  } finally {
    askYes = real;
  }
  t.expect(!$("styles-empty").hidden, "the empty-list text comes back");
  t.click($("styles-close"));
  await invoke("save_sidecar");
});

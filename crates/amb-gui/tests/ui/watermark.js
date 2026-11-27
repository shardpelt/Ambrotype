// SPDX-License-Identifier: GPL-3.0-or-later
// The Watermark maker: design on a live preview, save by name, pick it in
// Export's Saved list, delete it.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/watermark.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

const type = (id, value) => {
  const e = $(id);
  e.value = value;
  e.dispatchEvent(new Event("input", { bubbles: true }));
};

await t.step("the maker opens and shows the typed text on the photo", async () => {
  KEY_ACTIONS.find((a) => a.id === "watermark-maker").run();
  await t.until("the dialog", () => t.visible("#wmk-dialog"));
  t.expect(!t.visible("#wmk-preview"), "no preview before there is a mark");
  type("wmk-text", "© UI test");
  await t.until("the preview", () => t.visible("#wmk-preview") && $("wmk-preview").naturalWidth > 0, 60000);
});

await t.step("clicking the preview sets Place to the nearest of nine", async () => {
  const box = $("wmk-preview").getBoundingClientRect();
  $("wmk-preview").dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: box.left + 2, clientY: box.top + 2 }));
  t.expect($("wmk-anchor").value === "top-left", "top left");
  $("wmk-preview").dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: box.left + box.width / 2, clientY: box.top + box.height / 2 }));
  t.expect($("wmk-anchor").value === "center", "centre");
});

await t.step("Save keeps it under a name", async () => {
  type("wmk-name", "UI test mark");
  t.click($("wmk-save"));
  await t.until("it listed", () => [...$("wmk-saved").options].some((o) => o.value === "UI test mark"));
  const kept = (await invoke("watermarks")).find((w) => w.name === "UI test mark");
  t.expect(kept && kept.mark.text === "© UI test" && kept.mark.anchor === "center", "the saved mark");
  t.click($("wmk-close"));
  await t.until("the dialog closed", () => !t.visible("#wmk-dialog"));
});

await t.step("Export's Saved list has it and fills the fields", async () => {
  const flow = exportFlow();
  await t.until("the export dialog", () => t.visible("#export-dialog") && [...$("wm-saved").options].some((o) => o.value === "UI test mark"));
  $("wm-saved").value = "UI test mark";
  $("wm-saved").dispatchEvent(new Event("change", { bubbles: true }));
  const f = $("export-form");
  t.expect(f.wm_text.value === "© UI test" && f.wm_anchor.value === "center", "Export's fields");
  // Design… starts from Export's fields, and Use puts the change back.
  t.click($("wm-design"));
  await t.until("the maker", () => t.visible("#wmk-dialog") && $("wmk-text").value === "© UI test");
  t.expect(t.visible("#wmk-use"), "Use in Export is offered");
  type("wmk-text", "© Changed");
  t.click($("wmk-use"));
  await t.until("the maker closed", () => !t.visible("#wmk-dialog"));
  t.expect(f.wm_text.value === "© Changed", "Use filled Export");
  $("export-dialog").close("cancel");
  await flow;
});

await t.step("Delete removes it", async () => {
  const real = askYes;
  askYes = async () => true;
  try {
    KEY_ACTIONS.find((a) => a.id === "watermark-maker").run();
    await t.until("the dialog", () => t.visible("#wmk-dialog"));
    $("wmk-saved").value = "UI test mark";
    $("wmk-saved").dispatchEvent(new Event("change", { bubbles: true }));
    t.click($("wmk-delete"));
    await t.until("it gone", () => ![...$("wmk-saved").options].some((o) => o.value === "UI test mark"));
  } finally {
    askYes = real;
  }
  t.expect((await invoke("watermarks")).every((w) => w.name !== "UI test mark"), "gone from disk");
  t.click($("wmk-close"));
});

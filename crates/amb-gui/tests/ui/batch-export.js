// SPDX-License-Identifier: GPL-3.0-or-later
// Batch export: a folder with a subfolder to smaller copies, then again.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/batch-export.js photo.dng
// (with a folder "more" holding a photo next to it: amb testimage ramp more/other.dng)

await t.step("the dialog counts the photos in the folder and its subfolders", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await openFolder(dirOf(ui.info.path));
  await t.until("the grid", () => libraryOn() && lib.photos.length > 0);
  t.click("#lib-batch");
  await t.until("the dialog", () => $("batch-dialog").open);
  await t.until("the count", () => /^[\d,]+ photos? in [\d,]+ folders?$/.test($("batch-count").textContent));
  t.expect($("batch-sub").checked, "subfolders included by default");
  t.expect($("batch-from").textContent === dirOf(ui.info.path), "the folder shown");
  const n = Number($("batch-count").textContent.split(" ")[0].replace(/,/g, ""));
  t.expect(n >= 2 && $("batch-count").textContent.includes("folders"), $("batch-count").textContent);
  globalThis.batchPhotos = n;
});

await t.step("exporting shows progress and a summary with the sizes", async () => {
  t.click("#batch-pick");
  await t.until("the folder", () => batch.to.endsWith("batch"));
  t.expect(!$("batch-start").disabled, "Start is on once there is a folder");
  $("batch-value").value = 256;
  t.click("#batch-start");
  await t.until("the summary", () => t.visible("#batch-summary"), 120000);
  const text = $("batch-summary").textContent;
  t.expect(text.startsWith(`${batchPhotos} photos exported`) && text.includes("→"), text);
  t.expect(!document.querySelector("#batch-summary .fail"), "no failures");
  t.expect(t.visible("#batch-open"), "a button for the folder");
});

await t.step("the copies are where the photos were, below the output folder, and smaller", async () => {
  const src = dirOf(ui.info.path);
  for (const [from, to] of [[`${src}/test.dng`, `${batch.to}/test.jpg`], [`${src}/more/other.dng`, `${batch.to}/more/other.jpg`]]) {
    const before = (await invoke("photo_details", { path: from })).bytes;
    const after = (await invoke("photo_details", { path: to })).bytes;
    t.expect(after > 0 && after < before, `${to}: ${after} bytes, from ${before}`);
  }
});

await t.step("again with skip on does nothing new", async () => {
  t.click("#batch-start");
  await t.until("the summary", () => t.visible("#batch-summary") && $("batch-summary").textContent.includes("skipped"), 120000);
  t.expect($("batch-summary").textContent.startsWith("0 photos exported"), $("batch-summary").textContent);
  t.click("#batch-close");
  await t.until("closed", () => !$("batch-dialog").open);
});

await t.step("search finds it by resize and smaller", async () => {
  const hit = (q) => KEY_ACTIONS.some((a) => a.id === "batch-export" && `${a.label} ${a.hint ?? ""}`.toLowerCase().includes(q));
  t.expect(hit("resize") && hit("smaller"), "the key action's words");
});

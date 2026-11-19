// SPDX-License-Identifier: GPL-3.0-or-later
// AI Denoise: the model offered, then a new DNG per raw.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/denoise.js photo.dng

const first = await t.until("the photo", () => ui.info?.path, 60000);

await t.step("AI Denoise offers its model, then a new DNG per raw", async () => {
  await toggleLibrary(true);
  select(first, "only");
  await denoiseFlow();
  await t.until("the dialog", () => $("denoise-dialog").open);
  t.expect($("denoise-what").textContent.includes("Denoised.dng"), "says what it makes");
  const m = denoise.model;
  t.expect(m && m.task === "denoise", "the denoise model is listed");
  // Not installed in a test run: the download is offered and Denoise waits.
  t.expect(m.installed ? !$("denoise-go").disabled : (!$("denoise-install").hidden && $("denoise-go").disabled), "install first, then denoise");
  $("denoise-dialog").close();
});

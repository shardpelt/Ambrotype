// SPDX-License-Identifier: GPL-3.0-or-later
// The Frame: a caption of more than one line, in the preview and the border it exports.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/frame.js photo.dng

await t.step("a caption of two lines makes a taller bottom band and a two-line border caption", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
  await toggleFrame(true);
  const text = $("frame-text");
  t.expect(text.tagName === "TEXTAREA", "the caption field takes several lines");
  const band = () => {
    const img = $("preview");
    return parseFloat(getComputedStyle(img).paddingBottom) / img.getBoundingClientRect().height;
  };
  text.value = "one";
  text.dispatchEvent(new Event("input"));
  const one = band();
  text.value = "one\ntwo";
  text.dispatchEvent(new Event("input"));
  const two = band();
  t.expect(two > one * 1.3, `the band grows with the lines: ${one} → ${two}`);
  t.expect($("frame-label").textContent.includes("\n"), "the label shows both lines");
  t.expect(frameBorder().caption.text === "one\ntwo", "the border's caption keeps the line break");
  text.value = "";
  text.dispatchEvent(new Event("input"));
  await toggleFrame(false);
});

// SPDX-License-Identifier: GPL-3.0-or-later
// The vectorscope: a switch in the histogram panel draws the photo's colours as
// a wheel, it changes with the edit, and the choice is remembered.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/vectorscope.js photo.dng

// Pixels the scope drew in its own colours (not the grey frame): coloured and opaque enough.
const scopeInk = () => {
  const c = $("histogram");
  const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
  let n = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] > 128 && Math.max(d[i], d[i + 1], d[i + 2]) - Math.min(d[i], d[i + 1], d[i + 2]) > 40) n++;
  }
  return n;
};

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
  await t.until("a histogram", () => ui.histogram, 60000);
});

await t.step("the switch shows the vectorscope, and remembers it", async () => {
  t.expect(t.visible("#scope-switch") && $("scope-switch").textContent === "Vectorscope", "a switch that offers the vectorscope");
  t.click("#scope-switch");
  await t.until("the vectorscope mode", () => scope.mode === "vectorscope");
  await t.until("the data", () => scope.data && scope.data.length === SCOPE_SIZE * SCOPE_SIZE, 30000);
  t.expect(store.text("amb.scope") === "vectorscope", "the mode stored");
  t.expect($("scope-switch").textContent === "Histogram", "the switch offers the histogram now");
  await t.until("something drawn", () => scopeInk() > 0);
});

await t.step("the cloud grows when Saturation goes up", async () => {
  const before = scopeInk();
  const sum = scope.data.reduce((s, n) => s + n, 0);
  const src = $("preview").src;
  t.setSlider("Saturation", 80);
  await t.until("a new preview", () => $("preview").src !== src);
  await t.until("a wider cloud", () => scopeInk() > before, 30000);
  // A draft render while the slider moves is smaller: wait for the settled one.
  await t.until("the same pixels, counted again", () => scope.data.reduce((s, n) => s + n, 0) === sum, 30000);
});

await t.step("the colour under the pointer is a ring on the scope", async () => {
  const r = $("preview").getBoundingClientRect();
  $("preview").dispatchEvent(new MouseEvent("mousemove", { clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, bubbles: true }));
  await t.until("a colour under the pointer", () => scope.dot && scope.dot.length === 3);
});

await t.step("the switch goes back to the histogram", async () => {
  t.click("#scope-switch");
  await t.until("the histogram mode", () => scope.mode === "histogram");
  t.expect(store.text("amb.scope") === "histogram" && $("scope-switch").textContent === "Vectorscope", "stored and labelled");
});

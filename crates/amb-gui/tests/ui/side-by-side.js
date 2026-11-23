// SPDX-License-Identifier: GPL-3.0-or-later
// Before and after side by side, filling the view, and above and below
// (right-click Before / after): zooming and dragging move both halves.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/side-by-side.js photo.dng

const box = (id) => $(id).getBoundingClientRect();
const near = (a, b, tol) => Math.abs(a - b) <= tol;
// A half (side-by-side.js sbsPanes) in the window's pixels.
const pane = (which) => {
  const v = box("viewer");
  const p = sbsPanes()[which];
  return { left: v.left + p.x, top: v.top + p.y, right: v.left + p.x + p.w, bottom: v.top + p.y + p.h };
};
const inside = (r, p) => r.left >= p.left - 1 && r.top >= p.top - 1 && r.right <= p.right + 1 && r.bottom <= p.bottom + 1;
const covers = (r, p) => r.left <= p.left + 1 && r.top <= p.top + 1 && r.right >= p.right - 1 && r.bottom >= p.bottom - 1;
const runAction = (id) => KEY_ACTIONS.find((a) => a.id === id).run();

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("Right-click Before / after: Side by side shows both whole, before on the left", async () => {
  const b = $("btn-before");
  const r = b.getBoundingClientRect();
  b.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2, clientX: r.left + 4, clientY: r.top + 4 }));
  const items = [...document.querySelectorAll(".ctx-menu button")];
  const names = items.map((x) => x.textContent);
  t.expect(["Side by side", "Side by side, filling the view", "Above and below"].every((n) => names.includes(n)), `the three layouts in the menu: ${names}`);
  t.click(items.find((x) => x.textContent === "Side by side"));
  await t.until("both photos in their halves", () => {
    if (sbs.layout !== "side" || !t.visible("#preview") || !t.visible("#preview-before")) return false;
    const [a, bf] = [box("preview"), box("preview-before")];
    const aspect = $("preview").naturalWidth / $("preview").naturalHeight;
    const shaped = (x) => near(x.width / x.height, aspect, aspect * 0.02);
    return bf.right <= a.left && inside(bf, pane("before")) && inside(a, pane("after")) && shaped(a) && shaped(bf);
  }, 30000);
  t.expect($("btn-before").classList.contains("active"), "Before / after shown as on");
  t.expect(!document.querySelector(".ctx-menu"), "the menu closed");
  t.expect(!items.some((x) => x.classList.contains("on")), "no layout ticked while none is on");
  // Opened again, the menu ticks the layout that is on.
  b.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2, clientX: r.left + 4, clientY: r.top + 4 }));
  const ticked = [...document.querySelectorAll(".ctx-menu button.on")].map((x) => x.textContent);
  t.expect(ticked.length === 1 && ticked[0] === "Side by side" && document.querySelector(".ctx-menu button.on").getAttribute("aria-checked") === "true", `ticked: ${ticked}`);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  t.expect(!document.querySelector(".ctx-menu") && sbs.layout === "side", "Esc closes the menu and keeps the layout");
});

await t.step("Side by side, filling the view: each half filled; a drag on Before moves both", async () => {
  runAction("side-by-side-fill");
  await t.until("each half filled", () => sbs.layout === "fill" && t.visible("#preview-before")
    && covers(box("preview"), pane("after")) && covers(box("preview-before"), pane("before")), 30000);
  const [a0, b0] = [box("preview"), box("preview-before")];
  const p = pane("before");
  const at = (dx) => ({ bubbles: true, button: 0, pointerId: 1, isPrimary: true, clientX: (p.left + p.right) / 2 + dx, clientY: (p.top + p.bottom) / 2 });
  const img = $("preview-before");
  img.dispatchEvent(new PointerEvent("pointerdown", at(0)));
  img.dispatchEvent(new PointerEvent("pointermove", at(-30)));
  img.dispatchEvent(new PointerEvent("pointermove", at(-60)));
  img.dispatchEvent(new PointerEvent("pointerup", at(-60)));
  const [a1, b1] = [box("preview"), box("preview-before")];
  t.expect(near(a1.left - a0.left, -60, 2) && near(b1.left - b0.left, -60, 2), `both moved 60 px left: after ${a1.left - a0.left}, before ${b1.left - b0.left}`);
  t.expect(!ui.zoom.on, "a drag doesn't zoom");
});

await t.step("Zoomed to 100%, both halves show the same crop at the same place", async () => {
  t.key("Z");
  await t.until("both crops rendered and lined up", () => {
    const [a, b] = [$("preview"), $("preview-before")];
    if (!ui.zoom.on || !ui.zoom.meta || !sbs.before?.meta || a.style.visibility === "hidden" || b.hidden) return false;
    const [ra, rb, pa, pb] = [box("preview"), box("preview-before"), pane("after"), pane("before")];
    return a.naturalWidth === b.naturalWidth && a.naturalHeight === b.naturalHeight
      && near(ra.width, rb.width, 1) && near(ra.height, rb.height, 1)
      && near(ra.left - pa.left, rb.left - pb.left, 1) && near(ra.top - pa.top, rb.top - pb.top, 1);
  }, 60000);
  t.key("Z");
  await t.until("back to the filled halves", () => !ui.zoom.on && sbs.layout === "fill"
    && covers(box("preview"), pane("after")) && covers(box("preview-before"), pane("before")), 30000);
});

await t.step("Above and below: before above, after below", async () => {
  runAction("above-below");
  await t.until("before above after", () => {
    if (sbs.layout !== "stacked" || !t.visible("#preview-before")) return false;
    const [a, bf] = [box("preview"), box("preview-before")];
    return bf.bottom <= a.top && inside(bf, pane("before")) && inside(a, pane("after"));
  }, 30000);
});

await t.step("Esc: back to one photo", async () => {
  t.key("Escape");
  t.expect(!sbs.layout && !t.visible("#preview-before") && !$("btn-before").classList.contains("active"), "one photo, Before / after off");
  await t.until("the photo fills the viewer again", () => {
    const [a, v] = [box("preview"), box("viewer")];
    return t.visible("#preview") && (near(a.width, v.width, 2) || near(a.height, v.height, 2));
  }, 30000);
});

await t.step("A click on Before / after still shows the split, and again hides it", async () => {
  t.click("#btn-before");
  await t.until("the split", () => ui.split && t.visible("#split-bar") && t.visible("#preview-before"), 30000);
  t.click("#btn-before");
  await t.until("one photo", () => !ui.split && !t.visible("#split-bar") && !t.visible("#preview-before"));
  t.expect(!$("btn-before").classList.contains("active"), "Before / after off");
});

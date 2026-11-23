// SPDX-License-Identifier: GPL-3.0-or-later
// Peek on hover (a resting pointer shows the loupe) and Speed Edit (Ctrl+drag
// on the photo changes the last slider moved).
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/quick-wins.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

const photo = () => $("preview");
const rect = () => photo().getBoundingClientRect();
const mouse = (type, dx, dy, extra = {}) => {
  const r = rect();
  photo().dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, clientX: r.left + r.width / 2 + dx, clientY: r.top + r.height / 2 + dy, button: 0, ...extra }));
};

await t.step("Peek on hover is off by default: a resting pointer shows nothing", async () => {
  store.remove("amb.peek");
  mouse("mousemove", 0, 0);
  await t.sleep(PEEK_DELAY_MS + 400);
  t.expect(!qz.loupe, "no loupe");
});

await t.step("the settings box and the key action turn it on", async () => {
  t.expect(!$("settings-peek").checked, "the box starts off");
  t.expect(KEY_ACTIONS.some((a) => a.id === "peek-hover"), "the key action");
  hoverPeekSet(true);
  t.expect($("settings-peek").checked && store.text("amb.peek") === "1", "box and saved value on");
});

await t.step("on, a still pointer shows the loupe after the delay, and a move hides it", async () => {
  mouse("mousemove", 0, 0);
  await t.sleep(PEEK_DELAY_MS / 2);
  t.expect(!qz.loupe, "not yet");
  mouse("mousemove", 1, 1);
  await t.until("the loupe", () => qz.loupe && !qz.loupe.hidden);
  mouse("mousemove", PEEK_END_PX + 8, 0);
  t.expect(!qz.loupe, "gone after a move");
});

await t.step("a key press ends a peek, and Peek stays off while the loupe (I) is on", async () => {
  mouse("mousemove", 0, 0);
  await t.until("the loupe", () => qz.loupe);
  t.key("Shift");
  t.expect(!qz.loupe, "closed by a key");
  toggleLoupe(true);
  mouse("mousemove", 0, 0);
  await t.sleep(PEEK_DELAY_MS + 300);
  t.expect(qz.loupe && !hoverPeek.shown, "the loupe mode is left alone");
  toggleLoupe(false);
  hoverPeekSet(false);
});

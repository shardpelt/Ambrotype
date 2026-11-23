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

await t.step("Ctrl+drag on the photo with no slider moved yet only says how", async () => {
  speed.last = null;
  mouse("mousedown", 0, 0, { ctrlKey: true });
  mouse("mouseup", 100, 0, { ctrlKey: true });
  t.expect(!speed.drag, "no drag started");
  t.expect(KEY_ACTIONS.some((a) => a.id === "speed-edit"), "the key action");
});

await t.step("Ctrl+drag right on the photo raises the last slider, as one undo step", async () => {
  t.setSlider("Exposure2012", 0.5);
  await t.until("Exposure 0.5", () => t.setting("Exposure2012") === 0.5);
  const steps = t.historyLength();
  mouse("mousedown", 0, 0, { ctrlKey: true });
  window.dispatchEvent(new MouseEvent("mousemove", { clientX: rect().left + rect().width / 2 + 100, clientY: rect().top + 10, ctrlKey: true }));
  const live = ui.overrides.Exposure2012;
  t.expect(live > 0.5, `the preview value rose while dragging (${live})`);
  window.dispatchEvent(new MouseEvent("mouseup", { button: 0, ctrlKey: true }));
  await t.until("the setting rose", () => t.setting("Exposure2012") > 0.5);
  await t.until("one history step", () => t.historyLength() === steps + 1);
  t.expect(t.row("Exposure2012").querySelector(".value").value !== "+0.50", "the box shows the new value");
  t.key("Ctrl+Z");
  await t.until("undone", () => t.setting("Exposure2012") === 0.5);
});

await t.step("on/off menu items carry a tick that follows the toggle", async () => {
  const clip = KEY_ACTIONS.find((a) => a.id === "clipping");
  t.expect(typeof clip.isOn === "function", "Clipping overlay has isOn");
  t.expect(menuEntry("clipping", false).checked === false, "the entry starts unticked");
  t.expect(menuEntry("open", false).checked === undefined, "a plain action has no tick");
  // The native menu can't be read here: pretend it is up, then see what is sent.
  menuState.shown = true;
  menuState.checked = {};
  syncMenu();
  t.expect(menuState.checked.clipping === false, "unticked is recorded");
  clip.run({});
  syncMenu();
  t.expect(menuState.checked.clipping === true, "ticked after the toggle");
  clip.run({});
  syncMenu();
  t.expect(menuState.checked.clipping === false, "unticked again");
  menuState.shown = false;
});

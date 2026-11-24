// SPDX-License-Identifier: GPL-3.0-or-later
// Shortcut sets: loading a shared file applies the actions that exist and counts the rest.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/shortcuts.js photo.dng

await t.step("a shortcuts file with one known and one unknown action: the known one is kept, the unknown one counted", async () => {
  await t.until("the photo", () => ui.info, 60000);
  t.expect(KEY_ACTIONS.some((a) => a.id === "shortcuts-save") && KEY_ACTIONS.some((a) => a.id === "shortcuts-load"), "both are key actions");
  const before = { ...ui.keybindings };
  await keysApplySet({ "auto-edit": ["Ctrl+Alt+Q"], "no-such-action": ["Ctrl+Alt+W"] });
  t.expect(ui.keybindings["auto-edit"]?.join() === "Ctrl+Alt+Q", "the known action has the loaded key");
  t.expect(!("no-such-action" in ui.keybindings), "the unknown one is not kept");
  t.expect($("toast").textContent.includes("Loaded 1 shortcut") && $("toast").textContent.includes("Skipped 1 unknown action"), `the note: ${$("toast").textContent}`);
  t.expect((await invoke("keybindings"))["auto-edit"]?.join() === "Ctrl+Alt+Q", "saved with the settings");
  // Back to what it was.
  ui.keybindings = before;
  await saveKeybindings();
  t.expect(!("auto-edit" in (await invoke("keybindings"))), "back to the default");
});

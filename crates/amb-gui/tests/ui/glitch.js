// SPDX-License-Identifier: GPL-3.0-or-later
// Glitch, the task plugin: its tools put the glitch and halftone effects on,
// each rendered, and take them off.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/glitch.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("Glitch, a task plugin: its tools put effects on and take them off", async () => {
  t.expect([...$("task-bar").children].some((b) => b.dataset.task === "glitch-lab"), "a Glitch task from its plugin");
  await switchTask("glitch-lab");
  await t.until("its tools", () => t.visible("#block-tools") && $("tools-title").textContent === "Glitch tools");
  t.expect(ui.looks.active_layout === "plugin:glitch-lab", `layout ${ui.looks.active_layout}`);
  const tile = (label) => [...document.querySelectorAll("#tools-task .tool-tile")].find((b) => b.querySelector("strong").textContent === label);
  const fxOf = (plugin) => effectsOf(currentValue("amb:Effects")).find((e) => e.plugin === plugin);
  const steps = t.historyLength();
  t.click(tile("Tiles"));
  await t.until("Tiles", () => fxOf("glitch")?.params.style === "Tiles");
  t.expect(t.historyLength() === steps + 1, "one history step");
  t.click(tile("Game Boy"));
  await t.until("Game Boy", () => fxOf("halftone")?.params.ink === "Game Boy");
  await t.until("the preview", () => $("preview").complete && $("preview").naturalWidth > 0);
  t.expect(!(ui.state?.not_rendered ?? []).some((u) => /glitch|halftone/.test(u)), `rendered: ${ui.state?.not_rendered}`);
  t.click(tile("Back to normal"));
  await t.until("the effects off", () => !fxOf("glitch") && !fxOf("halftone"));
  while (ui.state.can_undo) await run([{ cmd: "undo" }]);
  await switchTask("develop");
});

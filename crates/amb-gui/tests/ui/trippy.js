// SPDX-License-Identifier: GPL-3.0-or-later
// Trippy, the task plugin: its tools put effects on and take them off.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/trippy.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("Trippy, a task plugin: its tools put effects on and take them off", async () => {
  t.expect([...$("task-bar").children].some((b) => b.dataset.task === "trippy"), "a Trippy task from its plugin");
  await switchTask("trippy");
  await t.until("its tools", () => t.visible("#block-tools") && $("tools-title").textContent === "Trippy tools");
  t.expect(ui.looks.active_layout === "plugin:trippy", `layout ${ui.looks.active_layout}`);
  const tile = (label) => [...document.querySelectorAll("#tools-task .tool-tile")].find((b) => b.querySelector("strong").textContent === label);
  const fxOn = () => effectsOf(currentValue("amb:Effects")).map((e) => e.plugin);
  const steps = t.historyLength();
  t.click(tile("Trip me"));
  await t.until("a trip", () => fxOn().some((p) => p === "hue-trip" || p === "holographic"));
  t.expect(t.historyLength() === steps + 1, "one history step");
  t.click(tile("Acid"));
  await t.until("Acid", () => effectsOf(currentValue("amb:Effects")).find((e) => e.plugin === "hue-trip")?.params.turn === 120);
  t.expect(fxOn().filter((p) => p === "hue-trip").length === 1, `one Hue trip: ${fxOn()}`);
  await t.until("History names the effects", () => /→ .*Hue trip/.test(document.querySelector("#history-list li.current .history-values")?.textContent ?? ""));
  t.expect(!document.querySelector("#history-list li.current .history-values").textContent.includes("{"), "no JSON in History");
  // Visualise's view as a look: Neon (Paint's edges glowing on black).
  t.click(tile("Neon"));
  await t.until("Neon", () => effectsOf(currentValue("amb:Effects")).find((e) => e.plugin === "paint")?.params.style === "Neon");
  // A kaleidoscope (it reads the whole photo) and heat-camera colour,
  // each rendered: the photo's warnings don't say "not rendered".
  t.click(tile("Fractal"));
  await t.until("Fractal", () => effectsOf(currentValue("amb:Effects")).find((e) => e.plugin === "kaleidoscope")?.params.style === "Fractal");
  t.click(tile("Heat camera"));
  await t.until("Heat camera", () => effectsOf(currentValue("amb:Effects")).some((e) => e.plugin === "heat-camera"));
  await t.until("the preview", () => $("preview").complete && $("preview").naturalWidth > 0);
  t.expect(!(ui.state?.not_rendered ?? []).some((u) => /kaleidoscope|heat-camera/.test(u)), `rendered: ${ui.state?.not_rendered}`);
  // Kept to the highlights and pushed past full; the Effects panel says
  // where, and hovering it shows where on the photo.
  t.click(tile("Acid highlights"));
  await t.until("Acid highlights", () => effectsOf(currentValue("amb:Effects")).some((e) => e.plugin === "hue-trip" && e.where === "highlights" && e.amount === 160));
  revealSetting("amb:Effects");
  await t.until("its Where", () => [...document.querySelectorAll(".fx-where select")].some((x) => x.value === "highlights"));
  const row = [...document.querySelectorAll(".fx-where")].find((r) => r.querySelector("select").value === "highlights");
  await t.until("the preview", () => $("preview").complete && $("preview").naturalWidth > 0);
  row.dispatchEvent(new MouseEvent("mouseenter"));
  t.expect(!$("where-canvas").hidden, "hovering Where shows where");
  row.dispatchEvent(new MouseEvent("mouseleave"));
  t.expect($("where-canvas").hidden, "and leaving hides it");
  t.expect([...document.querySelectorAll(".fx-effect input[type=range]")][0].max === "200", "Amount to 200 in Trippy");
  t.click(tile("Back to normal"));
  await t.until("no trip", () => fxOn().length === 0);
  // Hovering an effect button shows the photo with it, as presets do; nothing is applied.
  const before = t.historyLength();
  tile("Foil sticker").dispatchEvent(new MouseEvent("mouseenter"));
  await t.until("the hover preview", () => Object.keys(ui.overrides ?? {}).includes("amb:Effects"), 5000);
  tile("Foil sticker").dispatchEvent(new MouseEvent("mouseleave"));
  await t.until("the preview off again", () => !Object.keys(ui.overrides ?? {}).length, 3000);
  t.expect(t.historyLength() === before && fxOn().length === 0, "nothing applied");
  // Back to where the edit was, for the steps after this one.
  while (ui.state.can_undo) await run([{ cmd: "undo" }]);
  await t.until("History is empty", () => t.historyLength() === 0);
  // Not in the Appearance list: it is the task's.
  t.expect(TASK_LAYOUTS.has("plugin:trippy"), "kept out of Appearance");
  await switchTask("develop");
});

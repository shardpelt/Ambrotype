// SPDX-License-Identifier: GPL-3.0-or-later
// The Develop panels, drawn from the registry: in each, the first slider takes
// a value in its range as one history step, a double-click on its label puts it
// back, the panel folds and opens, and Search finds the slider by name.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/panels.js photo.dng

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

const steps0 = t.historyLength();

// The first slider of a panel that is drawn as a plain slider (not log-scaled,
// not another widget), with the panel's element.
const firstSlider = (panel) => {
  for (const s of panel.sections) {
    for (const def of s.controls) {
      const c = ui.controls.get(def.name);
      if (def.kind !== "number" || isLog(def) || !c?.input || !c.label?.isConnected || c.input.disabled) continue;
      return { def, c, details: c.label.closest("details.panel") };
    }
  }
  return null;
};

// A value inside the range, not the default, on the slider's step.
const pickValue = (def) => {
  const lo = def.min;
  const hi = def.max;
  const on = (v) => Math.min(hi, Math.max(lo, Number((lo + Math.round((v - lo) / def.step) * def.step).toFixed(6))));
  const dflt = Number(def.default ?? 0);
  const v = on(hi - (hi - lo) * 0.25);
  return v !== dflt ? v : on(lo + (hi - lo) * 0.25);
};

const near = (a, b, def) => Math.abs(a - b) <= def.step / 2 + 1e-6;
const panelsWithSlider = ui.panels.filter((p) => firstSlider(p));
t.expect(panelsWithSlider.length >= 5, `panels with a slider: ${panelsWithSlider.length}`);

for (const p of panelsWithSlider) {
  const { def, details } = firstSlider(p);
  await t.step(`${p.label}: ${def.label} sets, resets, and the panel folds`, async () => {
    const steps = t.historyLength();
    const was = Number(t.setting(def.name) ?? def.default ?? 0);
    const v = pickValue(def);
    t.expect(v >= def.min && v <= def.max && !near(v, was, def), `a new value ${v} (was ${was})`);
    revealSetting(def.name);
    t.expect(details.open, "the panel is open");
    t.setSlider(def.name, v);
    await t.until(`${def.name} follows`, () => near(Number(t.setting(def.name)), v, def), 30000);
    await t.until("a history step", () => t.historyLength() === steps + 1);
    t.expect(near(Number(t.row(def.name).querySelector("input.value").value), v, def), "the value box shows it");
    // Double-click on the label is the reset.
    t.row(def.name).querySelector("label").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await t.until("back to the default", () => near(Number(t.setting(def.name) ?? def.default ?? 0), Number(def.default ?? 0), def), 30000);
    await t.until("a history step for the reset", () => t.historyLength() === steps + 2);
    // Fold and open again.
    const summary = details.querySelector("summary");
    t.click(summary);
    await t.until("folded", () => !details.open);
    t.click(summary);
    await t.until("open again", () => details.open);
  });
}

await t.step("Search finds a slider by its name and takes you to it", async () => {
  const { def } = firstSlider(ui.panels.find((p) => p.id === "basic") ?? panelsWithSlider[0]);
  const word = window.t(def.label);
  t.click("#btn-search");
  await t.until("the search box", () => $("search-dialog").open);
  $("search-input").value = word;
  $("search-input").dispatchEvent(new Event("input"));
  const hit = searchState.shown.findIndex((it) => it.kind === "Setting" && it.title === word);
  t.expect(hit >= 0, `a Setting named ${word} among ${searchState.shown.map((it) => it.title).join(", ")}`);
  chooseSearch(hit);
  await t.until("the search box closed", () => !$("search-dialog").open);
  await t.until("the slider selected", () => ui.selectedSlider === def.name);
});

await t.step("the history is put back for the next script", async () => {
  while (t.historyLength() > steps0) {
    const len = t.historyLength();
    await t.sleep(150); // a key right after the last change can be lost
    t.key("Ctrl+Z");
    await t.until("one more undo", () => t.historyLength() < len);
  }
});

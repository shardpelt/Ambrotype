// SPDX-License-Identifier: GPL-3.0-or-later
// Import: the Copy / Move choice, a tick per photo with All and None, and
// Eject. Nothing is imported: the controls are checked, and the plan is
// asked for only the photos named.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/import.js photo.dng

const first = await t.until("the photo", () => ui.info?.path, 60000);
const source = dirOf(first);
const dest = `${source}/imported-here`;

await t.step("the dialog offers Copy or Move, and Eject", async () => {
  importFlow();
  await t.until("the dialog", () => $("import-dialog").open);
  const f = importForm();
  t.expect([...f.mode.options].map((o) => o.value).join() === "copy,move", "Copy and Move");
  t.expect(f.mode.value === "copy" || f.mode.value === "move", "one of them chosen");
  t.expect(f.eject.type === "checkbox", "Eject when done");
  f.source.value = source;
  f.dest.value = dest;
  imp.off.clear();
  planImport();
  await t.until("the photos listed", () => imp.plan.length >= 1);
});

await t.step("each new photo has a tick; None and All change what is imported", async () => {
  const boxes = [...$("import-list").querySelectorAll("input[type=checkbox]")];
  t.expect(boxes.length === imp.plan.filter((p) => !p.skip).length && boxes.length >= 1, "a tick per new photo");
  t.expect(boxes.every((b) => b.checked) && !$("import-go").disabled, "all ticked, Import ready");
  t.expect(!$("import-pick").hidden, "All and None shown");
  const all = importTicked();
  boxes[0].click();
  t.expect(imp.off.size === 1 && importTicked() === all - 1 && importPicked().length === all - 1, "unticking one leaves the others");
  boxes[0].click();
  t.expect(imp.off.size === 0 && importTicked() === all && !$("import-go").disabled, "ticking it again brings it back");
  $("import-none").click();
  t.expect(importTicked() === 0 && boxes.every((b) => !b.checked) && $("import-go").disabled, "None leaves nothing to import");
  $("import-all").click();
  t.expect(importTicked() === imp.plan.filter((p) => !p.skip).length && boxes.every((b) => b.checked), "All");
  t.expect(importPicked().length === 0, "all ticked means no list is sent");
});

await t.step("the plan can be asked for named photos only", async () => {
  const none = await invoke("import_plan", { source, dest, options: { ...importOptions(), only: ["nothing-like-this.nef"] } });
  t.expect(none.length === 0, "a name that is not there lists nothing");
  const named = await invoke("import_plan", { source, dest, options: { ...importOptions(), only: [fileName(first).toLowerCase()] } });
  t.expect(named.length === 1 && named[0].source === first, "a name, in any case, lists its photo");
});

await t.step("Stop is hidden until an import runs", async () => {
  t.expect($("import-stop").hidden, "no Stop yet");
  importRunning(true);
  t.expect(!$("import-stop").hidden && $("import-cancel").disabled, "Stop shown, Cancel off while running");
  importRunning(false);
  t.expect($("import-stop").hidden && !$("import-cancel").disabled, "back to normal");
  $("import-cancel").click();
  await t.until("the dialog closed", () => !$("import-dialog").open);
});

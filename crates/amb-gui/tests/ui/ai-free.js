// SPDX-License-Identifier: GPL-3.0-or-later
// AI-free mode: the switch shows a badge and turns the AI controls off (Subject
// mask, AI Remove, Select, Smart search), and turning it off gives them back.
// It leaves the mode off, as every script after it expects.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/ai-free.js photo.dng

await t.step("the photo opens; the mode starts off", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
  if (aiFree.on) await aiFreeSet(false);
  t.expect(!aiFree.on, "off");
});

await t.step("the one badge says No AI marks, then AI in edit when the edit holds an AI part", async () => {
  const badge = $("ai-free-badge");
  await t.until("the badge", () => !badge.hidden && badge.textContent === "No AI marks");
  t.expect(badge.title.includes("Platforms may also run their own checks"), "its title carries the caveat");
  const steps = t.historyLength();
  await run([{ cmd: "set_param", name: "EnhanceDenoiseVersion", value: "1" }]);
  await t.until("AI in edit", () => badge.textContent === "AI in edit");
  t.expect(badge.dataset.state === "edit" && badge.title.includes("Enhance"), `the reason: ${badge.title}`);
  aiFreeExportNote(true);
  t.expect(!$("export-ai-label").hidden && $("export-ai-label").textContent.startsWith("AI in edit"), "the export dialog says so too");
  await run([{ cmd: "undo" }]);
  await t.until("undone", () => t.historyLength() === steps && badge.textContent === "No AI marks");
});

await t.step("turning it on shows the badge and turns an AI mask button off", async () => {
  t.key("M");
  await t.until("the mask panel", () => mk.on && !$("mask-panel").hidden);
  const subject = () => [...$("mask-new").querySelectorAll("button")].find((b) => b.textContent === "Subject");
  t.expect(subject() && !subject().disabled, "Subject is on first");
  await aiFreeSet(true);
  t.expect(!$("ai-free-badge").hidden && $("ai-free-badge").textContent === "AI-free", "the badge says AI-free");
  t.expect($("ai-free-switch").checked, "the Settings switch is on");
  await t.until("Subject off", () => subject().disabled);
  t.expect(subject().title.startsWith("Off while AI-free mode is on"), "with its reason");
  const sky = [...$("mask-new").querySelectorAll("button")].find((b) => b.textContent === "Sky");
  t.expect(!sky.disabled, "Sky is not AI and stays");
  t.expect($("mask-words").disabled && $("btn-select").disabled, "Describe it and Select are off");
  t.expect(await invoke("ai_free_get"), "the editor says it is on");
});

await t.step("AI Remove is off in the spot panel, Heal is not", async () => {
  t.key("Q");
  await t.until("the spot tool", () => sp.on && !$("spot-panel").hidden);
  const kind = (k) => document.querySelector(`#spot-kind [data-kind="${k}"]`);
  t.expect(kind("remove").disabled && !kind("heal").disabled, "Remove off, Heal on");
  t.expect($("spot-object").disabled, "Remove object is off");
  exitSpots();
});

await t.step("an edit that already holds an AI part: the note names what is left out", async () => {
  await aiFreeSet(false);
  const steps = t.historyLength();
  await run([{ cmd: "set_param", name: "EnhanceDenoiseVersion", value: "1" }]);
  await t.until("the AI setting in the edit", () => t.historyLength() === steps + 1);
  t.expect($("ai-free-note").hidden, "no note while the mode is off");
  await aiFreeSet(true);
  t.expect(!$("ai-free-note").hidden && $("ai-free-note").textContent.includes("leaves out"), `the note: ${$("ai-free-note").textContent}`);
  // Undo with the mode off, so the edit is as it was.
  await aiFreeSet(false);
  await run([{ cmd: "undo" }]);
  await t.until("undone", () => t.historyLength() === steps);
  t.expect($("ai-free-note").hidden, "the note is gone");
  await aiFreeSet(true);
});

await t.step("the key action turns it off again and the controls come back", async () => {
  await aiFreeToggle();
  t.expect(!aiFree.on && $("ai-free-badge").textContent === "No AI marks", "off, back to the verdict");
  t.key("M");
  await t.until("the mask panel", () => mk.on && !$("mask-panel").hidden);
  const subject = [...$("mask-new").querySelectorAll("button")].find((b) => b.textContent === "Subject");
  t.expect(!subject.disabled && !$("mask-words").disabled, "Subject and Describe it are on");
  t.expect(!(await invoke("ai_free_get")), "the editor says it is off");
  t.key("M");
});

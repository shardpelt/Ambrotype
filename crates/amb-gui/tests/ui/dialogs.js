// SPDX-License-Identifier: GPL-3.0-or-later
// Every dialog opens, shows its main button, and closes.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/dialogs.js photo.dng

const first = await t.until("the photo", () => ui.info?.path, 60000);

// Opens with `open()` (a flow that may wait for the dialog to close), checks
// the main button is there, closes it, and checks it closed.
const opens = async (id, open, button) => t.step(`${id} opens and closes`, async () => {
  const p = open();
  await t.until(id, () => $(id).open, 10000);
  if (button) t.expect(document.querySelector(button) && !document.querySelector(button).hidden, `${button} is there`);
  $(id).close();
  await Promise.race([p, t.sleep(1000)]);
  t.expect(!$(id).open, `${id} closed`);
});

// A fresh config may have no presets; the dialog only shows the name, and
// closing it applies nothing.
ui.presets = await invoke("presets");
await opens("paste-dialog", () => applyPresetPartly(ui.presets[0] ?? { name: "A preset" }), "#paste-dialog button.primary");
await opens("surprise-dialog", () => surpriseFlow(), "#surprise-more");
await t.step("Surprise me: tone off leaves tone alone, a typed seed is the first look's seed", async () => {
  // The step before clicked More and left that roll running: its looks must not replace these.
  surpriseFlow();
  await t.until("the looks", () => $("surprise-grid").children.length === 6);
  $("surprise-tone").checked = false;
  $("surprise-seed").value = "42";
  $("surprise-seed").dispatchEvent(new Event("change"));
  await t.until("the seeded looks", () => $("surprise-grid").firstChild?.title === "Seed 42");
  const [v] = await invoke("surprise", { seed: 42, count: 1, params: { style: "Anything", strength: 80, colour: true, tone: false, finish: true } });
  t.expect(v.seed === 42, "seed 42");
  t.expect(!JSON.stringify(v.commands).includes("Contrast2012"), "no Contrast2012");
  $("surprise-dialog").close("cancel");
  $("surprise-seed").value = "";
  $("surprise-tone").checked = true;
});
await opens("shortcuts-dialog", () => showShortcuts(), "#shortcuts-dialog button.primary");
await opens("settings-dialog", () => settingsFlow(), '#settings-dialog [data-tab="keys"]');
await opens("sizes-dialog", () => sizesFlow(), "#sizes-new");
await opens("basics-dialog", () => basicsFlow());
await opens("learn-dialog", () => learnFlow());
await opens("assistant-dialog", () => assistantFlow(), "#assistant-send");
await opens("plugins-dialog", () => pluginsFlow(), "#plugin-run");
// It asks the hub over the network: only that the dialog opens is checked.
await opens("hub-dialog", () => hubFlow(), '#hub-dialog [data-kind="all"]');
await opens("name-dialog", () => askName("Name"), "#name-dialog button.primary");
await opens("smart-dialog", () => editSmartCollection(null), "#smart-save");
await opens("lrcat-dialog", () => lrcatFlow(), "#lrcat-pick");

await t.step("the library shows the photo", async () => {
  await toggleLibrary(true);
  await t.until("the grid", () => libPhoto(first));
  select(first, "only");
});
await opens("meta-dialog", () => metaTemplatesFlow(), "#meta-apply");
await opens("import-dialog", () => importFlow());

// The test folder has one raw, so the merges say what they need instead of opening.
for (const [id, merge] of [["hdr-dialog", libraryMergeHdr], ["pano-dialog", libraryMergePano]]) {
  await t.step(`${id}: one photo selected, it asks for two and stays closed`, async () => {
    select(first, "only");
    await merge();
    t.expect(!$(id).open, `${id} stays closed`);
    t.expect(/two or more/i.test($("toast")?.textContent ?? document.body.textContent), "it says to select two or more");
  });
}

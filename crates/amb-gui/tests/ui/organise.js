// SPDX-License-Identifier: GPL-3.0-or-later
// Library → Organise: rename the open photo (it reopens under its new
// name), undo, and the duplicates, keywords and missing-photo dialogs.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/organise.js photo.dng

const first = await t.until("the photo", () => ui.info?.path, 60000);

await t.step("the library shows the photo", async () => {
  await toggleLibrary(true);
  await t.until("the grid", () => libPhoto(first));
  select(first, "only");
});

await t.step("Rename shows the new name, then renames and reopens it", async () => {
  t.click("#lib-organise-menu summary");
  t.click("#lib-rename");
  await t.until("the dialog", () => $("files-dialog").open);
  $("files-template").value = "renamed-{seq}";
  await planFiles();
  t.expect(org.plan.length === 1 && org.plan[0].to.endsWith("renamed-0001.dng"), "the new name in the plan");
  t.click("#files-go");
  await t.until("the open photo renamed", () => ui.info?.path.endsWith("renamed-0001.dng"));
});

await t.step("Undo puts the name back", async () => {
  await undoMove();
  await t.until("the old name", () => ui.info?.path === first);
});

await t.step("one photo has no duplicates", async () => {
  t.click("#lib-duplicates");
  t.click("#dupes-find");
  await t.until("an answer", () => $("dupes-status").textContent.startsWith("Nothing"), 60000);
  $("dupes-dialog").close();
});

await t.step("Keywords and Find missing photos open", async () => {
  t.click("#lib-keywords");
  await t.until("the keyword list", () => $("keywords-dialog").open && !$("keywords-list").textContent.includes("Reading"));
  $("keywords-dialog").close();
  t.click("#lib-missing");
  await t.until("the answer", () => $("missing-status").textContent && !$("missing-status").textContent.startsWith("Looking"));
  t.log($("missing-status").textContent);
  $("missing-dialog").close();
});

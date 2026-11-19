// SPDX-License-Identifier: GPL-3.0-or-later
// Clean up: a rejected photo is found; K keeps it, D lets it go.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/cleanup.js photo.dng

const first = await t.until("the photo", () => ui.info?.path, 60000);

await t.step("the library shows the photo", async () => {
  await toggleLibrary(true);
  await t.until("the grid", () => libPhoto(first));
  select(first, "only");
});

await t.step("Clean up finds a rejected photo; K keeps it, D lets it go", async () => {
  await setInfo([first], { flag: "reject" });
  t.click("#lib-organise-menu summary");
  t.click("#lib-cleanup");
  await t.until("the dialog", () => $("clean-dialog").open);
  for (const box of $("clean-tests").querySelectorAll("input")) box.checked = box.value === "rejected";
  t.click("#clean-find");
  await t.until("the review", () => !$("clean-review").hidden, 60000);
  t.expect(clean.found.length === 1 && clean.found[0].reasons[0].kind === "rejected", "the rejected photo, with why");
  t.expect($("clean-count").textContent.startsWith("1 of 1"), "marked to go at first");
  $("clean-dialog").dispatchEvent(new KeyboardEvent("keydown", { key: "k", bubbles: true }));
  t.expect($("clean-count").textContent.startsWith("0 of 1") && $("clean-trash").disabled, "K keeps it");
  $("clean-dialog").dispatchEvent(new KeyboardEvent("keydown", { key: "d", bubbles: true }));
  t.expect($("clean-count").textContent.startsWith("1 of 1"), "D lets it go again");
  $("clean-dialog").close();
  await setInfo([first], { flag: "" });
});

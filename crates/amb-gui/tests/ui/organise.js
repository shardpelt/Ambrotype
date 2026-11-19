// SPDX-License-Identifier: GPL-3.0-or-later
// Library → Organise: rename the open photo (it reopens under its new
// name), undo, and the duplicates, keywords and missing-photo dialogs.
// Clean up, Connect an AI app and AI Denoise have their own scripts.
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

await t.step("dragging a photo onto a folder moves it, Undo brings it back", async () => {
  select(first, "only");
  const dir = first.replace(/[\\/][^\\/]*$/, "");
  const dest = `${dir}/more`;
  // The panel lists the library folders, not a folder opened alone: show these.
  const saved = { roots: tree.roots, folders: tree.folders, open: new Set(tree.open) };
  tree.roots = [{ path: dir, offline: false }];
  tree.folders = [{ folder: dest, photos: 1 }];
  tree.open = new Set([dir]);
  renderFolderTree();
  const row = await t.until("the more folder row", () => [...$("tree-list").querySelectorAll(".tree-row")].find((r) => r.dataset.path === dest));
  const data = new DataTransfer();
  data.setData("text/x-amb-photo", first);
  const over = new DragEvent("dragover", { dataTransfer: data, bubbles: true, cancelable: true });
  row.dispatchEvent(over);
  t.expect(over.defaultPrevented && row.classList.contains("drop-target"), "the row accepts the photo");
  row.dispatchEvent(new DragEvent("drop", { dataTransfer: data, bubbles: true, cancelable: true }));
  await t.until("the photo in more", () => ui.info?.path === `${dest}/test.dng` && !libPhoto(first));
  t.expect(!row.classList.contains("drop-target"), "the highlight gone");
  t.expect(org.last?.length >= 1 && org.last.every(([, to]) => to.startsWith(dest)) && !$("lib-undo-move").hidden, "every file of the photo moved, Undo offered");
  await undoMove();
  await t.until("the photo back", () => ui.info?.path === first && libPhoto(first));
  Object.assign(tree, saved);
  renderFolderTree();
});

await t.step("one photo has no duplicates", async () => {
  t.click("#lib-duplicates");
  t.click("#dupes-find");
  await t.until("an answer", () => $("dupes-status").textContent.startsWith("Nothing"), 60000);
  $("dupes-dialog").close();
});

await t.step("Keywords and Find missing photos open", async () => {
  t.click("#lib-keyword-list");
  await t.until("the keyword list", () => $("keywords-dialog").open && !$("keywords-list").textContent.includes("Reading"));
  $("keywords-dialog").close();
  t.click("#lib-missing");
  await t.until("the answer", () => $("missing-status").textContent && !$("missing-status").textContent.startsWith("Looking"));
  t.log($("missing-status").textContent);
  $("missing-dialog").close();
});

await t.step("recent keywords add to the selection in one click", async () => {
  select(first, "only");
  await libraryChange({ add_keywords: ["ui-test"] });
  await libraryChange({ remove_keywords: ["ui-test"] });
  const chip = await t.until("the recent chip", () => [...document.querySelectorAll("#lib-tag-recent .tag-recent")].find((b) => b.textContent === "+ ui-test"));
  chip.click();
  await t.until("the keyword back", () => libPhoto(first).keywords.includes("ui-test"));
  t.click("#lib-keyword-list");
  await t.until("the list", () => [...document.querySelectorAll("#keywords-list input")].some((b) => b.value === "ui-test"));
  $("keywords-dialog").close();
  await libraryChange({ remove_keywords: ["ui-test"] });
});

await t.step("the top bar hides and comes back at the top edge", async () => {
  const bar = document.querySelector(".topbar");
  t.key("Ctrl+Shift+T");
  await t.until("hidden", () => bar.getBoundingClientRect().bottom <= 1);
  $("topbar-edge").dispatchEvent(new MouseEvent("mouseenter"));
  await t.until("shown while pointed at", () => bar.getBoundingClientRect().top >= 0);
  t.key("Ctrl+Shift+T");
  await t.until("back", () => !document.body.classList.contains("topbar-off"));
  t.expect($("settings-topbar").checked, "Settings to say it shows");
});

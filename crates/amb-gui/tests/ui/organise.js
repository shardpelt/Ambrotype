// SPDX-License-Identifier: GPL-3.0-or-later
// Library → Organise: rename the open photo (it reopens under its new
// name), undo, Clean up, and the duplicates, keywords and missing-photo
// dialogs; then Connect an AI app's guides and agent button.
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

await t.step("Connect an AI app has the built-in guides and starts an agent", async () => {
  await connectFlow();
  t.expect($("connect-guide").options.length >= 10 && $("connect-guide").value === "guide", "the guides, the one to read first first");
  const claude = connect.apps.findIndex((a) => a.agent?.startsWith("claude"));
  $("connect-app").value = String(claude);
  $("connect-app").dispatchEvent(new Event("change"));
  t.expect(!$("connect-agent").hidden && $("connect-agent").textContent === "Start Claude Code here", `the agent button: ${$("connect-agent").textContent}`);
  $("connect-app").value = "0";
  $("connect-app").dispatchEvent(new Event("change"));
  t.expect($("connect-agent").hidden, "no agent button for an MCP app");
  t.click("#connect-read");
  await t.until("the guide", () => $("guide-dialog").open);
  t.expect($("guide-text").textContent.startsWith("# Driving Ambrotype"), "the guide's text");
  $("guide-dialog").close();
  $("connect-dialog").close();
});

await t.step("AI Denoise offers its model, then a new DNG per raw", async () => {
  await toggleLibrary(true);
  select(first, "only");
  await denoiseFlow();
  await t.until("the dialog", () => $("denoise-dialog").open);
  t.expect($("denoise-what").textContent.includes("Denoised.dng"), "says what it makes");
  const m = denoise.model;
  t.expect(m && m.task === "denoise", "the denoise model is listed");
  // Not installed in a test run: the download is offered and Denoise waits.
  t.expect(m.installed ? !$("denoise-go").disabled : (!$("denoise-install").hidden && $("denoise-go").disabled), "install first, then denoise");
  $("denoise-dialog").close();
});

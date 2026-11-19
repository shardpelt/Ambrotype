// SPDX-License-Identifier: GPL-3.0-or-later
// Connect an AI app: the built-in guides, and an agent started.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/connect.js photo.dng

const first = await t.until("the photo", () => ui.info?.path, 60000);

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

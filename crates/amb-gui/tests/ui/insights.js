// SPDX-License-Identifier: GPL-3.0-or-later
// Insights and Timeline: a bar's filter comes off again (chip, Back, Esc);
// zoom by Ctrl + wheel and Ctrl + / − / 0; a chart enlarges on a click.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/insights.js photo.dng

await t.step("Open folder shows the photo in the library", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await openFolder(dirOf(ui.info.path));
  await t.until("the grid", () => libraryOn() && lib.photos.length > 0);
});

const showView = async (view, wait) => {
  t.click(`#lib-views [data-view="${view}"]`);
  await t.until(`the ${view}`, () => cull.view === view && document.querySelector(wait));
};

await t.step("a bar's filter comes off with the chip, which says so", async () => {
  await showView("insights", ".in-charts");
  const all = lib.shown.length;
  const bar = document.querySelector(".in-bar:not(:disabled), .in-row");
  t.expect(bar, "a bar to click");
  bar.click();
  await t.until("the grid with the filter", () => cull.view === "grid" && lib.pick);
  t.expect(t.visible("#lib-pick") && $("lib-pick").textContent.includes("Show all"), `the chip says how: "${$("lib-pick").textContent}"`);
  t.click("#lib-pick");
  await t.until("all again", () => !lib.pick && $("lib-pick").hidden);
  t.expect(lib.shown.length === all, `the full count is back (${lib.shown.length} of ${all})`);
});

await t.step("Back returns to Insights without the filter", async () => {
  await showView("insights", ".in-charts");
  document.querySelector(".in-bar:not(:disabled), .in-row").click();
  await t.until("the grid with the filter", () => cull.view === "grid" && lib.pick);
  navBack();
  await t.until("Insights again", () => cull.view === "insights" && !lib.pick && libraryOn());
  t.expect($("lib-pick").hidden, "the chip is gone");
});

await t.step("Esc in the grid takes the filter off, and a second Esc leaves", async () => {
  await showView("insights", ".in-charts");
  document.querySelector(".in-bar:not(:disabled), .in-row").click();
  await t.until("the grid with the filter", () => cull.view === "grid" && lib.pick);
  t.key("Escape");
  t.expect(!lib.pick && $("lib-pick").hidden && libraryOn(), "the filter is off, still in the library");
});

await t.step("a date filter from Timeline undoes with Back too", async () => {
  await showView("timeline", ".tl-months, .browse-empty");
  const day = document.querySelector(".tl-day.has");
  if (!day) return t.log("no dated photo, skipped");
  day.click();
  await t.until("the grid with the date", () => cull.view === "grid" && lib.date);
  t.expect(t.visible("#lib-date") && $("lib-date").textContent.includes("Show all"), "the chip says how");
  navBack();
  await t.until("Timeline again", () => cull.view === "timeline" && !lib.date);
});

const wheel = (deltaY, over) => over.dispatchEvent(new WheelEvent("wheel", { deltaY, ctrlKey: true, bubbles: true, cancelable: true }));
const scale = () => Number($("library-view").style.getPropertyValue("--browse-scale"));

await t.step("Timeline zooms by Ctrl + wheel and Ctrl + / − / 0, and remembers", async () => {
  await showView("timeline", ".tl-months, .browse-empty");
  t.key("Ctrl+0");
  t.expect(scale() === 1, `starts at 1 (${scale()})`);
  t.key("Ctrl+=");
  t.expect(scale() > 1, `Ctrl+= grows (${scale()})`);
  t.expect(store.text("amb.browseScale.timeline") === String(scale()), "remembered");
  const was = scale();
  wheel(100, $("library-view"));
  t.expect(scale() < was, `Ctrl + wheel shrinks (${scale()})`);
  for (let i = 0; i < 80; i++) wheel(-100, $("library-view"));
  t.expect(scale() === 2.5, `stops at the largest (${scale()})`);
  for (let i = 0; i < 200; i++) wheel(100, $("library-view"));
  t.expect(scale() === 0.6, `stops at the smallest (${scale()})`);
  t.key("Ctrl+0");
  t.expect(scale() === 1, "Ctrl+0 resets");
});

await t.step("Insights zooms on its own scale", async () => {
  await showView("insights", ".in-charts");
  t.key("Ctrl+0");
  t.key("Ctrl+=");
  const s = scale();
  t.expect(s > 1 && store.text("amb.browseScale.insights") === String(s), `bigger (${s}) and remembered`);
  await showView("timeline", ".tl-months, .browse-empty");
  t.expect(scale() === 1, "Timeline keeps its own scale");
  await showView("insights", ".in-charts");
  t.expect(scale() === s, "Insights kept its own");
  t.key("Ctrl+0");
  t.expect(scale() === 1, "Ctrl+0 resets");
});

await t.step("a click on a chart's title enlarges it; Esc, its × or the room round it closes", async () => {
  await showView("insights", ".in-charts");
  const total = document.querySelectorAll(".in-card:not(.in-advice)").length;
  t.expect(total >= 5, `a chart each (${total})`);
  document.querySelector(".in-charts .in-card h4").click();
  t.expect(insights.big && document.querySelectorAll(".in-charts .in-card").length === 1, "one chart shown");
  t.expect(document.querySelector(".in-charts.big .in-bar:not(:disabled), .in-charts.big .in-row"), "its bars are still there to click");
  t.key("Escape");
  t.expect(!insights.big && document.querySelectorAll(".in-charts .in-card").length === total, "Esc shows them all again");
  t.expect(cull.view === "insights", "still in Insights");
  document.querySelector(".in-charts .in-card h4").click();
  document.querySelector(".in-close").click();
  t.expect(!insights.big, "the × closes");
  document.querySelector(".in-charts .in-card h4").click();
  $("library-view").querySelector(".in-charts.big").click();
  t.expect(!insights.big, "a click on the room round the chart closes");
});

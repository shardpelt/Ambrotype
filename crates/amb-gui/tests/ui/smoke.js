// SPDX-License-Identifier: GPL-3.0-or-later
// The editor's main path: open, edit, undo, redo, a panel's tabs, help, export.
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/smoke.js photo.dng
// (src/uitest.rs; the helpers are in ui/uitest.js).

await t.step("the photo opens and a preview shows", async () => {
  await t.until("the photo", () => ui.info, 60000);
  await t.until("the preview", () => t.visible("#preview") && $("preview").naturalWidth > 0, 60000);
});

await t.step("the first start asks nothing: no dialog over the photo", async () => {
  await t.sleep(1500);
  const open = [...document.querySelectorAll("dialog[open]")].map((d) => d.id);
  t.expect(!open.length && !ui.asked, `open: ${open.join(", ")}; system questions: ${ui.asked ?? 0}`);
});

const before = t.setting("Exposure2012") ?? 0;
await t.step("a slider edits the photo, as one history step", async () => {
  const steps = t.historyLength();
  const src = $("preview").src;
  t.setSlider("Exposure2012", 1);
  await t.until("Exposure +1", () => t.setting("Exposure2012") === 1);
  await t.until("a history step", () => t.historyLength() === steps + 1);
  await t.until("a new preview", () => $("preview").src !== src);
  t.expect(t.row("Exposure2012").querySelector(".value").value === "+1.00", "the box to show +1.00");
});

await t.step("Ctrl+Z undoes and Ctrl+Shift+Z redoes", async () => {
  t.key("Ctrl+Z");
  await t.until("Exposure back", () => (t.setting("Exposure2012") ?? 0) === before);
  t.key("Ctrl+Shift+Z");
  await t.until("Exposure +1 again", () => t.setting("Exposure2012") === 1);
});

await t.step("search: the Search button opens it; a setting's name and a number sets it", async () => {
  t.expect(t.visible("#btn-search"), "a Search button in the toolbar");
  t.click("#btn-search");
  await t.until("the search box", () => $("search-dialog").open);
  $("search-input").value = "contrast 25";
  $("search-input").dispatchEvent(new Event("input"));
  t.expect(searchState.shown[0]?.title === "Set Contrast to +25", `first: ${searchState.shown[0]?.title}`);
  t.key("Enter");
  await t.until("Contrast +25", () => t.setting("Contrast2012") === 25);
  await run([{ cmd: "undo" }]);
  await t.until("Contrast back", () => !("Contrast2012" in ui.settings));
});

await t.step("Auto edit: the big button edits the whole photo, as one undo step", async () => {
  t.expect(t.visible("#btn-auto") && !$("btn-auto").disabled, "an Auto button in the toolbar");
  t.expect($("btn-auto").textContent.includes("Auto"), "the button keeps its word");
  const steps = t.historyLength();
  t.click("#btn-auto");
  await t.until("Auto edit", () => t.setting("Vibrance") === 15, 60000);
  await t.until("one history step", () => t.historyLength() === steps + 1);
  t.expect(t.setting("LensProfileEnable") === true && t.setting("AutoLateralCA") === true, "lens corrections on");
  t.expect(t.setting("Exposure2012") !== 1, "the tone set afresh");
  await run([{ cmd: "undo" }]);
  await t.until("all of it undone", () => !t.setting("Vibrance") && t.setting("Exposure2012") === 1);
});

await t.step("tasks differ: Develop opens on Quick fixes, Retouch on its tools, Color on colour only", async () => {
  await switchTask("develop");
  await t.until("Quick fixes", () => t.visible("#block-quick") && !t.visible("#block-retouch"));
  t.expect(document.body.dataset.task === "develop", `accent ${document.body.dataset.task}`);
  await switchTask("retouch");
  await t.until("Retouch tools", () => t.visible("#block-retouch") && !t.visible("#block-quick"));
  const tiles = [...document.querySelectorAll("#tools-retouch .tool-tile")].map((b) => b.dataset.tool);
  t.expect(["select", "object", "remove", "subject", "people", "background", "sky", "dust"].every((x) => tiles.includes(x)), tiles.join(", "));
  t.expect(document.querySelector("#tools-retouch .tool-tile").dataset.tool === "select" && document.querySelector("#tools-retouch .tool-tile").classList.contains("big"), "Select object first, big");
  // Select object: O opens it, its actions wait for a selection, Esc leaves.
  document.querySelector('#tools-retouch [data-tool="select"]').click();
  await t.until("Select object", () => sl.on && t.visible("#select-panel"));
  t.expect([...document.querySelectorAll("#select-actions button")].every((b) => b.disabled), "nothing to act on yet");
  sl.dabs = [[0.5, 0.5, 0.05]];
  renderSelectPanel();
  t.expect([...document.querySelectorAll("#select-actions button")].every((b) => !b.disabled), "the actions once something is selected");
  t.expect($("select-shadow-row").hidden, "no shadow offered when none was found");
  sl.shadowDabs = [[0.5, 0.6, 0.02]];
  renderSelectPanel();
  t.expect(t.visible("#select-shadow-row") && !$("select-shadow").checked && $("select-hint").textContent.includes("shadow"), "a shadow found is offered");
  sl.shadowDabs = [];
  renderSelectPanel();
  // Its actions also sit right by the selection, on the photo.
  t.expect(t.visible("#select-bar"), "the action bar by the selection");
  const bar = $("select-bar").getBoundingClientRect();
  const view = $("select-canvas").getBoundingClientRect();
  t.expect(bar.left >= view.left && bar.right <= view.right && bar.top >= view.top && bar.bottom <= view.bottom, "on the photo's view");
  $("select-bar").querySelector('[data-bar="move"]').click();
  t.expect(sl.moving, "the bar's Move");
  $("select-bar").querySelector('[data-bar="move"]').click();
  t.expect(!sl.moving, "and again stops it");
  t.click("#select-move");
  t.expect(sl.moving && $("select-hint").textContent.startsWith("Drag"), "Move waits for a drag");
  t.key("Escape");
  t.expect(!sl.moving && sl.on, "Esc stops moving first");
  t.key("Escape");
  t.expect(!sl.on && $("select-panel").hidden && $("select-bar").hidden, "then leaves");
  // The toolbar's Select button (in every built-in layout) opens it too.
  t.expect(t.visible("#btn-select") && $("btn-select").textContent.includes("Select"), "Select in the toolbar, with its word");
  t.click("#btn-select");
  await t.until("the toolbar's Select", () => sl.on);
  t.click("#btn-select");
  t.expect(!sl.on, "and again leaves");
  t.key("O");
  await t.until("O opens it", () => sl.on);
  // Typing what: words without the search model say where to get it; a
  // word for a mask the app makes goes straight to it.
  t.expect(t.visible("#select-words"), "a box to type what");
  $("select-words").value = "a red car";
  $("toast").hidden = true;
  $("select-words").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await t.until("the note", () => !$("toast").hidden && !sl.busy);
  t.expect($("toast").textContent.includes("search model") && sl.on, `the note: ${$("toast").textContent}`);
  t.expect(maskWord("the sky") === "sky" && maskWord("people") === "people" && maskWord("a car") === null, "mask words");
  t.key("Escape");
  // Its tiles open the tools.
  document.querySelector('#tools-retouch [data-tool="object"]').click();
  await t.until("box mode", () => sp.on && sp.object);
  t.key("Escape");
  t.key("Escape");
  await t.until("spots closed", () => !sp.on);
  document.querySelector('#tools-retouch [data-tool="brush"]').click();
  await t.until("masks with the brush", () => mk.on && mk.tool === "brush");
  exitMasks();
  await switchTask("color");
  await t.until("the Color layout", () => document.body.dataset.task === "color");
  const shown = (name) => { try { return t.visible(t.row(name)); } catch { return false; } };
  t.expect(!shown("Exposure2012") && shown("Vibrance"), "colour only in Basic");
  await switchTask("develop");
  await t.until("Develop back", () => shown("Exposure2012"));
});

await t.step("tips: a feature to try, skipped once used, with switches in Settings", async () => {
  await loadFeatures();
  t.expect(!window.__ambTipShownAtStart && $("tip-card").hidden, "no tip during tests by itself");
  const tip = pickTip();
  t.expect(tip?.f.want && tip.target.key, "a tip to show");
  t.expect(showTip(tip) && t.visible("#tip-card") && $("tip-want").textContent === tip.f.want, "the card shows it");
  // Not now hides it; a used command isn't offered again.
  t.click("#tip-close");
  t.expect($("tip-card").hidden, "hidden");
  markUsed(tip.target.key);
  for (let i = 0; i < 20; i++) t.expect(pickTip()?.target.key !== tip.target.key, "used ones skipped");
  // The search box can suggest what to type.
  const random = Math.random;
  Math.random = () => 0.1;
  boxTip($("search-input"), "Search");
  Math.random = random;
  t.expect(/try “/.test($("search-input").placeholder), $("search-input").placeholder);
  // The switches.
  t.expect($("settings-tips-wait").checked && $("settings-tips-boxes").checked, "both on by default");
  $("settings-tips-boxes").checked = false;
  $("settings-tips-boxes").dispatchEvent(new Event("change"));
  boxTip($("search-input"), "Search");
  t.expect($("search-input").placeholder === "Search" && !tipsOn("boxes"), "off: the plain hint");
  $("settings-tips-boxes").checked = true;
  $("settings-tips-boxes").dispatchEvent(new Event("change"));
});

await t.step("Help → Camera simulator: the settings change the picture and say what they do", async () => {
  camsimFlow();
  await t.until("the simulator", () => $("camsim-dialog").open && $("camsim-notes").textContent.startsWith("Exposure"));
  t.expect($("camsim-notes").textContent.includes("well exposed"), `sunny 16: ${$("camsim-notes").textContent}`);
  const pixels = () => $("camsim-canvas").getContext("2d").getImageData(0, 0, 640, 400).data;
  const sum = (d) => d.reduce((a, v) => a + v, 0);
  const before = sum(pixels());
  // A slow shutter: brighter, the ball blurs, and Auto exposure brings it back.
  $("camsim-shutter").value = "7";
  $("camsim-shutter").dispatchEvent(new Event("input"));
  await t.until("1/30 s", () => $("camsim-shutter-v").textContent === "1/30 s");
  t.expect(sum(pixels()) > before, "brighter");
  t.expect(/too bright/.test($("camsim-notes").textContent) && /motion blur/.test($("camsim-notes").textContent), $("camsim-notes").textContent);
  t.click("#camsim-auto");
  await t.until("well exposed again", () => $("camsim-notes").textContent.includes("well exposed"));
  $("camsim-dialog").close();
});

await t.step("Trippy, a task plugin: its tools put effects on and take them off", async () => {
  t.expect([...$("task-bar").children].some((b) => b.dataset.task === "trippy"), "a Trippy task from its plugin");
  await switchTask("trippy");
  await t.until("its tools", () => t.visible("#block-tools") && $("tools-title").textContent === "Trippy tools");
  t.expect(ui.looks.active_layout === "plugin:trippy", `layout ${ui.looks.active_layout}`);
  const tile = (label) => [...document.querySelectorAll("#tools-task .tool-tile")].find((b) => b.querySelector("strong").textContent === label);
  const fxOn = () => effectsOf(currentValue("amb:Effects")).map((e) => e.plugin);
  const steps = t.historyLength();
  t.click(tile("Trip me"));
  await t.until("a trip", () => fxOn().some((p) => p === "hue-trip" || p === "holographic"));
  t.expect(t.historyLength() === steps + 1, "one history step");
  t.click(tile("Acid"));
  await t.until("Acid", () => effectsOf(currentValue("amb:Effects")).find((e) => e.plugin === "hue-trip")?.params.turn === 120);
  t.expect(fxOn().filter((p) => p === "hue-trip").length === 1, `one Hue trip: ${fxOn()}`);
  await t.until("History names the effects", () => /→ .*Hue trip/.test(document.querySelector("#history-list li.current .history-values")?.textContent ?? ""));
  t.expect(!document.querySelector("#history-list li.current .history-values").textContent.includes("{"), "no JSON in History");
  // Visualise's view as a look: Neon (Paint's edges glowing on black).
  t.click(tile("Neon"));
  await t.until("Neon", () => effectsOf(currentValue("amb:Effects")).find((e) => e.plugin === "paint")?.params.style === "Neon");
  // Kept to the highlights and pushed past full; the Effects panel says
  // where, and hovering it shows where on the photo.
  t.click(tile("Acid highlights"));
  await t.until("Acid highlights", () => effectsOf(currentValue("amb:Effects")).some((e) => e.plugin === "hue-trip" && e.where === "highlights" && e.amount === 160));
  revealSetting("amb:Effects");
  await t.until("its Where", () => [...document.querySelectorAll(".fx-where select")].some((x) => x.value === "highlights"));
  const row = [...document.querySelectorAll(".fx-where")].find((r) => r.querySelector("select").value === "highlights");
  await t.until("the preview", () => $("preview").complete && $("preview").naturalWidth > 0);
  row.dispatchEvent(new MouseEvent("mouseenter"));
  t.expect(!$("where-canvas").hidden, "hovering Where shows where");
  row.dispatchEvent(new MouseEvent("mouseleave"));
  t.expect($("where-canvas").hidden, "and leaving hides it");
  t.expect([...document.querySelectorAll(".fx-effect input[type=range]")][0].max === "200", "Amount to 200 in Trippy");
  t.click(tile("Back to normal"));
  await t.until("no trip", () => fxOn().length === 0);
  // Back to where the edit was, for the steps after this one.
  for (let i = 0; i < 5; i++) await run([{ cmd: "undo" }]);
  await t.until("back at the Exposure step", () => document.querySelector("#history-list li.current .history-values")?.textContent.endsWith("→ +1.00"));
  // Not in the Appearance list: it is the task's.
  t.expect(TASK_LAYOUTS.has("plugin:trippy"), "kept out of Appearance");
  await switchTask("develop");
});

await t.step("search: what you want to do finds the feature, with a button for it", async () => {
  searchFlow();
  await t.until("the features list", () => ui.features?.length > 50);
  const type = (text) => {
    $("search-input").value = text;
    $("search-input").dispatchEvent(new Event("input"));
  };
  type("whatsapp");
  t.expect(searchState.shown[0]?.kind === "How to", `first: ${searchState.shown[0]?.kind} ${searchState.shown[0]?.title}`);
  type("raw by mistake");
  const hit = searchState.shown[0];
  t.expect(hit?.title.includes("RAW by mistake"), `first: ${hit?.title}`);
  t.key("Enter");
  await t.until("the How to", () => $("how-dialog").open);
  t.expect($("how-text").querySelector("code")?.textContent.startsWith("amb convert"), $("how-text").textContent);
  const buttons = [...$("how-actions").querySelectorAll("button")].map((b) => b.dataset.key);
  t.expect(buttons.includes("a:convert-raws"), `its buttons: ${buttons.join(", ")}`);
  // One answer leads to more: related features, each opening its own How to.
  const also = [...$("how-also").querySelectorAll("button")];
  t.expect(!$("how-also").hidden && also.length >= 2 && also.length <= 3, `related: ${also.map((b) => b.textContent).join(" | ")}`);
  const next = also[0].textContent;
  also[0].click();
  t.expect($("how-dialog").open && $("how-want").textContent === next, `followed to: ${$("how-want").textContent}`);
  // All features…: the whole list, filtered, and a row opens its How to.
  t.click("#how-all");
  await t.until("the features list", () => $("features-dialog").open);
  t.expect($("features-list").querySelectorAll(".feature-row").length > 50, "every feature listed");
  $("features-filter").value = "dust";
  $("features-filter").dispatchEvent(new Event("input"));
  const rows = [...$("features-list").querySelectorAll(".feature-row")];
  t.expect(rows.length >= 1 && rows.length < 5, `filtered to ${rows.length}`);
  rows[0].click();
  await t.until("its How to", () => $("how-dialog").open);
  t.expect($("how-all").hidden, "no All features… inside the list");
  $("how-dialog").close();
  $("features-dialog").close();
});

await t.step("Effects → Glow: Halation is a slider, one undoable step", async () => {
  revealSetting("amb:Halation");
  t.expect(t.visible(t.row("amb:Halation")) && t.visible(t.row("amb:Bloom")), "Halation and Bloom shown");
  t.expect(t.visible(t.row("amb:GrainFilm")), "Grain's Film response shown");
  t.expect(t.visible(t.row("amb:HighlightRolloff")), "Highlight roll-off shown");
  revealSetting("amb:DefringeAuto");
  t.expect(t.visible(t.row("amb:DefringeAuto")), "Defringe's Auto shown");
  const src = $("preview").src;
  t.setSlider("amb:Halation", 60);
  await t.until("Halation 60", () => t.setting("amb:Halation") === 60);
  await t.until("a new preview", () => $("preview").src !== src);
  await run([{ cmd: "undo" }]);
  await t.until("Halation off", () => !("amb:Halation" in ui.settings));
  revealSetting("Exposure2012");
});

await t.step("Ctrl+Shift+N makes a preset from the photo's edit", async () => {
  document.activeElement?.blur();
  t.key("Ctrl+Shift+N");
  await t.until("the dialog", () => $("save-preset-dialog").open);
  $("save-preset-dialog").close("cancel");
  t.expect(KEY_ACTIONS.some((a) => a.id === "save-preset") && MENU_BAR.flat(3).includes("save-preset"), "in the menu bar too");
});

await t.step("Convert raws from the editor: the open photo's folder, like this photo", async () => {
  const flow = convertRawsFlow();
  await t.until("the dialog", () => $("convert-dialog").open);
  t.expect($("convert-what").textContent.includes("in this folder"), $("convert-what").textContent);
  t.expect(!$("convert-like").disabled, "Like this photo offered");
  $("convert-dialog").close("cancel");
  await flow;
  t.expect(MENU_BAR.flat(3).includes("convert-raws"), "in the File menu");
});

await t.step("Negative: Auto Density and Auto Grade are switches beside Auto", async () => {
  revealSetting("amb:NegativeAutoDensity");
  t.expect(t.visible(t.row("amb:NegativeAutoDensity")) && t.visible(t.row("amb:NegativeAutoGrade")), "both shown");
  revealSetting("Exposure2012");
});

await t.step("filmstrip: Ctrl-click selects; right-click acts on the selection", async () => {
  const cell = strip.cells.get(ui.info.path);
  cell.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true }));
  t.expect(cell.classList.contains("selected") && strip.selected.size === 1, "selected");
  t.expect($("filmstrip-count").textContent === "1 selected", $("filmstrip-count").textContent);
  t.expect(selectedPhotos().length === 1, "actions see it");
  cell.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 700 }));
  const items = [...document.querySelectorAll(".ctx-menu button")].map((b) => b.textContent);
  t.expect(["Apply a preset…", "Export…", "Convert to JPEG…"].every((x) => items.includes(x)), items.join(", "));
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  document.querySelector(".ctx-menu")?.remove();
  cell.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  t.expect(strip.selected.size === 0 && !cell.classList.contains("selected"), "a plain click clears it");
});

await t.step("File → Export Edit Replay makes a video of the History", async () => {
  t.setSlider("Exposure2012", 0.5);
  await t.until("Exposure +0.5", () => t.setting("Exposure2012") === 0.5);
  const r = await invoke("export_replay", { dir: null, longEdge: 320 });
  t.expect(r && r.steps >= 1 && r.frames > 30 && r.bytes > 1000 && r.path.endsWith(".mp4"), JSON.stringify(r));
  t.expect(MENU_BAR.flat(3).includes("replay"), "in the File menu");
  await run([{ cmd: "undo" }]);
  await t.until("Exposure back", () => t.setting("Exposure2012") !== 0.5);
});

await t.step("History shows the step's values before and after", async () => {
  const values = () => document.querySelector("#history-list li.current .history-values")?.textContent ?? "";
  await t.until("the values", () => values().endsWith("→ +1.00"));
  t.expect(document.querySelector("#history-list li.current").title.includes("Exposure: "), "the tooltip to name Exposure");
});

await t.step("every panel opens", async () => {
  for (const d of document.querySelectorAll("details.panel")) {
    if (!d.open) t.click(d.querySelector("summary"));
    await t.sleep(30);
  }
  t.expect(t.visible(t.row("HueAdjustmentRed")), "the Color Mixer open");
});

await t.step("the right column's tabs show one group of panels at a time", async () => {
  const tabs = [...document.querySelectorAll("#col-right .column-tab")];
  t.expect(tabs.length >= 2, `tabs: ${tabs.map((b) => b.textContent).join(", ")}`);
  const shown = () => [...document.querySelectorAll("#col-right > details.panel")].filter((d) => !d.classList.contains("tab-off")).map((d) => d.dataset.tab);
  t.click(tabs[1]);
  t.expect(shown().length && shown().every((n) => n === tabs[1].dataset.tab), "only the second tab's panels");
  // Search finds a setting in another tab and brings that tab forward.
  revealSetting("Exposure2012");
  t.expect(t.visible(t.row("Exposure2012")), "Exposure shown again");
});

await t.step("no status bar; scrolling on the photo can step through the folder", async () => {
  t.expect(document.body.classList.contains("status-off") && document.querySelector(".statusbar").getBoundingClientRect().height === 0, "the status bar hidden");
  status("A message");
  t.expect(!$("status-note").hidden, "a message shows over the photo");
  const real = stepPhoto;
  let stepped = 0;
  stepPhoto = (d) => (stepped += d);
  const wheelAt = () => $("viewer").dispatchEvent(new WheelEvent("wheel", { deltaY: 100, bubbles: true, cancelable: true }));
  wheelAt();
  t.expect(stepped === 0, "off by default");
  $("settings-wheel-photos").checked = true;
  $("settings-wheel-photos").dispatchEvent(new Event("change"));
  wheelAt();
  stepPhoto = real;
  $("settings-wheel-photos").checked = false;
  $("settings-wheel-photos").dispatchEvent(new Event("change"));
  t.expect(stepped === 1, "on, scrolling down goes to the next photo");
});

await t.step("a section's buttons never cover the row under them, even in a narrow column", async () => {
  const col = $("col-right");
  const was = col.style.width;
  col.style.width = "260px";
  for (const d of col.querySelectorAll("details.panel")) d.open = true;
  await new Promise((r) => requestAnimationFrame(r));
  const bad = [];
  for (const label of col.querySelectorAll(".section-label")) {
    const next = label.nextElementSibling;
    if (!next || !label.querySelector(".section-action")) continue;
    const bottom = Math.max(...[...label.querySelectorAll(".section-action")].map((b) => b.getBoundingClientRect().bottom));
    if (bottom > next.getBoundingClientRect().top + 1) bad.push(label.querySelector(".section-name").textContent);
  }
  col.style.width = was;
  t.expect(!bad.length, `buttons over the next row in: ${bad.join(", ")}`);
});

await t.step("the Color Mixer shows one tab at a time", async () => {
  t.expect(!t.visible(t.row("SaturationAdjustmentRed")), "Saturation rows hidden on the Hue tab");
  const tab = [...document.querySelectorAll(".tabs button")].find((b) => b.textContent === "Saturation");
  t.click(tab);
  t.expect(t.visible(t.row("SaturationAdjustmentRed")), "Saturation rows shown");
  t.expect(!t.visible(t.row("HueAdjustmentRed")), "Hue rows hidden");
});

await t.step("a ? opens its help card", async () => {
  t.click(t.row("Exposure2012").querySelector(".help-mark"));
  await t.until("the card", () => help.card && t.visible(help.card));
  t.expect(help.card.textContent.includes("Exposure"), "the card to name Exposure");
  t.key("Escape");
});

await t.step("Settings says how the monitor's colours are handled", async () => {
  const s = await t.invoke("set_monitor_profile", { choice: { mode: "off" } });
  t.expect(s.choice.mode === "off" && $("settings-monitor-note").textContent !== "", "a monitor note");
  await t.invoke("set_monitor_profile", { choice: { mode: "auto" } });
  t.log(s.note);
});

await t.step("a preset's notes for scripts become one short line", async () => {
  const said = notesForPeople([
    "copied 23 setting(s) unknown to the registry without validation: A, B, C",
    "copied Temperature, Tint unscaled at 50%: no current value",
    "WhiteBalance set to \"Custom\" so that Temperature takes effect",
  ]);
  t.expect(said === "", `nothing to say: ${said}`);
  t.expect(notesForPeople(["no change"]) === "No change", "a no-op says so");
});

await t.step("the filmstrip's stars rate the open photo", async () => {
  const star = document.querySelector('#strip-rating [data-stars="3"]');
  t.click(star);
  await t.until("rated", () => infoState.info.rating === 3, 10000);
  t.expect(star.classList.contains("on") && !document.querySelector('#strip-rating [data-stars="4"]').classList.contains("on"), "three stars lit");
  t.click(star);
  await t.until("cleared", () => !infoState.info.rating, 10000);
});

await t.step("a filmstrip photo's stars rate it", async () => {
  const stars = strip.cells.get(ui.info.path).querySelector(".film-stars");
  t.expect(stars.classList.contains("editable") && stars.children.length === 5, "five stars to click");
  stars.querySelector('[data-stars="2"]').click();
  await t.until("rated", () => infoState.info.rating === 2, 10000);
  t.expect(stars.querySelectorAll(".on").length === 2, "two lit");
  stars.querySelector('[data-stars="2"]').click();
  await t.until("cleared", () => !infoState.info.rating, 10000);
});

await t.step("the whole photo fills the viewer", async () => {
  const img = $("preview").getBoundingClientRect();
  const view = $("viewer").getBoundingClientRect();
  t.expect(Math.abs(img.width - view.width) <= 2 || Math.abs(img.height - view.height) <= 2, `photo ${Math.round(img.width)}×${Math.round(img.height)} in ${Math.round(view.width)}×${Math.round(view.height)}`);
});

await t.step("zooming in shows the photo at once; a drag never shows black", async () => {
  setZoom(true, 0.3, 0.3);
  t.expect(!$("zoom-under").hidden && $("viewer").classList.contains("actual"), "the enlarged photo shows before the 100% crop arrives");
  await t.until("the 100% view", () => ui.zoom.meta, 60000);
  t.expect($("preview").style.visibility === "", "the sharp crop on top");
  placeRegion(-400, -300);
  const under = $("zoom-under").getBoundingClientRect();
  const view = $("viewer").getBoundingClientRect();
  t.expect(under.left <= view.left + 1 && under.top <= view.top + 1 && under.right >= view.right - 1 && under.bottom >= view.bottom - 1, "photo under the whole view while dragging");
  placeRegion(0, 0);
});

await t.step("the crop tool opened while zoomed in starts from the view", async () => {
  t.key("r");
  await t.until("the crop tool", () => ui.crop.on);
  const r = ui.crop.tool.rect();
  t.expect(r.width < ui.info.width * 0.95 || r.height < ui.info.height * 0.95, `a crop smaller than the photo: ${Math.round(r.width)}×${Math.round(r.height)}`);
  t.key("Escape");
  await t.until("the crop tool closed", () => !ui.crop.on);
  t.expect(!ui.zoom.on && $("zoom-under").hidden, "back to the whole photo");
});

await t.step("dragging the filmstrip moves along it and doesn't open a photo", async () => {
  const list = $("filmstrip-list");
  const spacer = Object.assign(document.createElement("div"), { style: "flex: none; width: 3000px" });
  list.append(spacer);
  list.scrollLeft = 0;
  const cell = list.querySelector(".film-cell") ?? list.firstElementChild;
  const at = cell.getBoundingClientRect();
  let opened = false;
  const real = openPath;
  openPath = async () => (opened = true);
  const ev = (type, x) => cell.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: at.top + 10, pointerId: 7, button: 0, pointerType: "mouse" }));
  ev("pointerdown", at.left + 40);
  ev("pointermove", at.left + 20);
  ev("pointermove", at.left - 160);
  ev("pointerup", at.left - 160);
  cell.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  openPath = real;
  spacer.remove();
  t.expect(list.scrollLeft > 100 || list.scrollWidth <= list.clientWidth, `moved along: ${list.scrollLeft}`);
  t.expect(!opened, "no photo opened");
});

await t.step("turning in the crop tool snaps to the photo's straight lines", async () => {
  t.key("r");
  await t.until("the crop tool", () => ui.crop.on);
  const lines = await t.invoke("guide_lines");
  t.expect(lines.length > 0, "the colour chart has straight lines");
  ui.crop.tool.setGuides(lines);
  ui.crop.tool.setDrawMode(false);
  const c = $("crop-canvas");
  const r = c.getBoundingClientRect();
  const [cx, cy, R] = [r.left + r.width / 2, r.top + r.height / 2, r.width / 2 - 3];
  const at = (deg, alt = false) => ({ bubbles: true, clientX: cx + R * Math.cos((deg * Math.PI) / 180), clientY: cy + R * Math.sin((deg * Math.PI) / 180), pointerId: 4, button: 0, altKey: alt });
  c.dispatchEvent(new PointerEvent("pointerdown", at(0)));
  c.dispatchEvent(new PointerEvent("pointermove", at(0.3)));
  t.expect(Math.abs(ui.crop.tool.rect().angle) < 0.05, `a small turn snaps back level: ${ui.crop.tool.rect().angle}`);
  c.dispatchEvent(new PointerEvent("pointermove", at(0.3, true)));
  t.expect(Math.abs(ui.crop.tool.rect().angle) > 0.2, `Alt turns freely: ${ui.crop.tool.rect().angle}`);
  c.dispatchEvent(new PointerEvent("pointerup", at(0.3, true)));
  t.key("Escape");
  await t.until("the crop tool closed", () => !ui.crop.on);
});

await t.step("Level in the crop tool keeps a level photo level", async () => {
  t.key("r");
  await t.until("the crop tool", () => ui.crop.on);
  t.click("#crop-level");
  await t.until("Level done", () => !$("crop-level").disabled && $("crop-angle-num").value !== "", 30000);
  t.expect(Math.abs(Number($("crop-angle").value)) < 0.5, `the colour chart is level already: ${$("crop-angle").value}`);
  t.key("Escape");
  await t.until("the crop tool closed", () => !ui.crop.on);
});

await t.step("a click near the photo's corner zooms in without showing past its edge", async () => {
  setZoom(true, 0.001, 0.999);
  await t.until("the 100% view", () => ui.zoom.meta, 60000);
  const img = $("preview").getBoundingClientRect();
  const view = $("viewer").getBoundingClientRect();
  t.expect(img.left <= view.left + 1 && img.bottom >= view.bottom - 1, `no void: photo ${Math.round(img.left)},${Math.round(img.bottom)} viewer ${Math.round(view.left)},${Math.round(view.bottom)}`);
  t.expect(ui.zoom.cx > 0.001 && ui.zoom.cy < 0.999, "the centre moved in from the corner");
  setZoom(false);
  await t.until("the whole photo", () => !$("viewer").classList.contains("actual"));
});

await t.step("Minimalism hides the chrome; the panels slide in from the edge", async () => {
  const before = { layout: ui.looks.active_layout, theme: ui.looks.active_theme };
  Object.assign(ui.looks, { active_layout: "builtin:minimalism", active_theme: "builtin:minimalism" });
  applyActiveLooks();
  const body = document.body.classList;
  t.expect(body.contains("chrome-hidden") && body.contains("topbar-off"), "the chrome hidden");
  t.expect(!$("edge-right").hidden && $("filmstrip").hidden, "a right edge strip, no filmstrip");
  t.key("Ctrl+]");
  t.expect(body.contains("peek-right"), "Ctrl+] slides the right column in");
  t.key("Ctrl+]");
  t.expect(!body.contains("peek-right"), "and out");
  // The pointer leaving it hides it again, even with a panel open.
  peekColumn("right", true);
  t.expect($("col-right").querySelector("details[open]"), "an open panel");
  $("col-right").dispatchEvent(new MouseEvent("mouseleave"));
  await t.until("the column hides after the pointer leaves", () => !body.contains("peek-right"), 3000);
  Object.assign(ui.looks, { active_layout: before.layout, active_theme: before.theme });
  applyActiveLooks();
  t.expect(!body.contains("chrome-hidden") && $("edge-right").hidden, "the chrome back");
});

await t.step("a group's 2 × 2 is a square of its best four", async () => {
  await collageFlow();
  await t.until("the collage maker", () => $("collage-dialog").open && collage.templates.length);
  const p = ui.info.path;
  collage.tray = [{ path: p, name: "a" }, { path: `${p}#2`, name: "b" }];
  collage.groups = [[p, `${p}#2`]];
  renderTray();
  t.expect(document.querySelector(".tray-group button")?.textContent === "2 × 2", "a 2 × 2 button on the group");
  t.expect($("col-group").textContent === "Ungroup" && !$("col-batch-groups-row").hidden, "Ungroup, and pages per group");
  groupGrid([p, p, p, p, p]);
  t.expect(collage.t.cells.length === 4 && collage.t.width === collage.t.height, "a square 2 × 2");
  t.expect(collage.cells.length === 4 && collage.cells.every((c) => c.path === p), "four photos in it");
  t.click("#col-no-borders");
  t.expect(collage.t.gap === 0 && collage.t.margin === 0 && $("col-gap-out").textContent === "0%", "no borders");
  $("col-batch-groups").checked = true;
  updateBatchNote();
  t.expect($("col-batch-note").textContent.includes("fewer than 4"), $("col-batch-note").textContent);
  setTray([]);
  closeCollage();
});

await t.step("Save to… keeps a preset in a folder of yours; a preset can go to the hub", async () => {
  t.setSlider("Exposure2012", 0.5);
  const flow = savePresetFlow("Shared look");
  await t.until("the Save dialog", () => $("save-preset-dialog").open);
  $("save-preset-dialog").close("to");
  await flow;
  const p = ui.presets.find((x) => x.name === "Shared look");
  t.expect(p && p.path.includes("ambrotype-ui-test"), `saved in the folder chosen: ${p?.path}`);
  // Its right-click menu offers the hub and the file.
  const row = [...document.querySelectorAll("#preset-list .preset")].find((b) => b.textContent === "Shared look");
  row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
  const items = [...document.querySelectorAll(".ctx-menu button")].map((b) => b.textContent);
  t.expect(items.includes("Share to the hub…") && items.includes("Show the file"), items.join(", "));
  document.querySelector(".ctx-menu")?.remove();
  t.key("Ctrl+Z");
});

await t.step("applying a preset shows no note, and its button no tooltip", async () => {
  // The preset the step before saved (a fresh setup has no other).
  const b = [...document.querySelectorAll("#preset-list .preset")].find((x) => x.textContent === "Shared look");
  const p = ui.presets.find((x) => x.name === "Shared look");
  t.expect(b && p, "the saved preset in the list");
  t.expect(!b.title, `no tooltip on the preset: ${b.title}`);
  $("toast").hidden = true;
  await applyPreset(p);
  await t.sleep(300);
  t.expect($("toast").hidden, `no note: ${$("toast").textContent}`);
  t.key("Ctrl+Z");
});

await t.step("Fuji recipe: a pasted recipe becomes a preset in the Fuji group, put on the photo", async () => {
  const flow = fujiRecipeFlow();
  await t.until("the dialog", () => $("fuji-dialog").open);
  $("fuji-text").value = "Test Roll\nFilm Simulation: Classic Chrome\nGrain Effect: Weak, Small\nWhite Balance: Auto, +2 Red & -4 Blue";
  $("fuji-sensor").value = "X-Trans IV";
  $("fuji-dialog").close("ok");
  await flow;
  const p = ui.presets.find((x) => x.name === "Test Roll");
  t.expect(p?.group === "Fuji", `in the Fuji group: ${JSON.stringify(p)}`);
  await t.until("it on the photo", () => ui.state?.settings?.["amb:WBShiftBlue"] === -4);
  t.key("Ctrl+Z");
});

await t.step("Y splits before and after; the bar drags", async () => {
  t.setSlider("Exposure2012", 1);
  // Keys typed in a slider's number box are the number's, not shortcuts.
  document.activeElement?.blur();
  t.key("Y");
  t.expect(ui.split, "Y turns the split on");
  await t.until("the before half", () => !$("preview-before").hidden && !$("split-bar").hidden, 20000);
  // The two halves line up once the photo has settled at its size.
  const lined = () => {
    const [p, b] = [$("preview").getBoundingClientRect(), $("preview-before").getBoundingClientRect()];
    return Math.abs(p.left - b.left) < 2 && Math.abs(p.width - b.width) < 2;
  };
  await t.until("the halves line up", lined, 10000);
  const p = $("preview").getBoundingClientRect();
  const bar = $("split-bar");
  const y = p.top + p.height / 2;
  bar.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: bar.getBoundingClientRect().left + 12, clientY: y, pointerId: 1 }));
  bar.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: p.left + p.width * 0.25, clientY: y, pointerId: 1 }));
  bar.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerId: 1 }));
  const at = bar.getBoundingClientRect();
  t.expect(Math.abs(at.left + at.width / 2 - (p.left + p.width * 0.25)) < 3, `the bar followed: ${at.left}`);
  t.key("Y");
  t.expect($("split-bar").hidden, "Y again hides it");
  // Before / after: the split with the bar near the right edge, to drag in.
  t.key("\\");
  await t.until("the bar at the side", () => !$("split-bar").hidden && !$("preview-before").hidden, 20000);
  const side = $("split-bar").getBoundingClientRect();
  const q = $("preview").getBoundingClientRect();
  t.expect(side.left > q.left + q.width * 0.85, `near the right edge: ${side.left} of ${q.left}+${q.width}`);
  t.expect($("btn-before").classList.contains("active") && !ui.before, "the button lit, the split shown");
  t.key("\\");
  t.expect($("split-bar").hidden && !$("btn-before").classList.contains("active"), "again: back to the edit");
  t.key("Ctrl+Z");
});

await t.step("Start from scratch: a blank layout, filled from Add; Cancel puts the old one back", async () => {
  startEditing(true);
  t.expect(!$("edit-palette").hidden, "Add is open");
  const visible = (id) => { const el = $(id); return el && !el.hidden && el.offsetParent !== null; };
  t.expect(!visible("btn-crop") && !visible("block-histogram"), "no buttons or blocks yet");
  addFromPalette({ add: "block", name: "histogram" });
  t.expect(visible("block-histogram"), "the histogram was added");
  await finishEditing(false);
  t.expect(visible("btn-crop"), "the old layout is back");
});

await t.step("Touch (T): a drag up on the photo changes what is under it, as one step", async () => {
  document.activeElement?.blur();
  t.key("T");
  t.expect(touchState.on && !$("touch-bar").hidden, "Touch is on");
  // What a drag would change, by colour.
  t.expect(touchTargets([0.05, 0.05, 0.05], "tone").targets[0][0] === "ParametricShadows", "dark: shadows");
  t.expect(touchTargets([0.95, 0.95, 0.95], "tone").targets[0][0] === "ParametricHighlights", "bright: highlights");
  const orange = touchTargets([0.9, 0.5, 0.1], "saturation").targets.map(([n]) => n);
  t.expect(orange.includes("SaturationAdjustmentOrange"), `orange: ${orange}`);
  const img = $("preview");
  await t.until("the photo", () => img.naturalWidth > 0 && !img.hidden);
  const r = img.getBoundingClientRect();
  const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
  img.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0, clientX: x, clientY: y }));
  window.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: x, clientY: y - 60 }));
  window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, clientX: x, clientY: y - 60 }));
  await t.until("a curve region moved", () => Object.keys(ui.settings).some((n) => n.startsWith("Parametric")));
  t.expect(!ui.zoom.on, "the drag didn't zoom");
  t.key("Ctrl+Z");
  t.key("Escape");
  t.expect(!touchState.on, "Esc stops it");
});

await t.step("Share before and after makes one web page", async () => {
  t.setSlider("Exposure2012", 0.7);
  $("toast").hidden = true;
  await shareCompareFlow();
  t.expect($("toast").textContent.includes("before-after.html"), `the note: ${$("toast").textContent}`);
  t.key("Ctrl+Z");
});

await t.step("right-click on the photo: Copy image puts it on the clipboard", async () => {
  photoMenu(new MouseEvent("contextmenu", { clientX: 400, clientY: 300 }));
  const items = [...document.querySelectorAll(".ctx-menu button")].map((b) => b.textContent);
  t.expect(["Copy image", "Copy file path", "Show in folder", "Export…", "Share before and after…"].every((i) => items.includes(i)), items.join(", "));
  document.querySelector(".ctx-menu")?.remove();
  const [w, h] = await t.invoke("copy_image", { path: ui.info.path });
  t.expect(w > 0 && h > 0 && Math.max(w, h) <= 3072, `copied ${w}×${h}`);
});

await t.step("Pick base: a click on the photo stores the film base; Alt-click forgets it", async () => {
  const pick = document.querySelector(".pick-base");
  t.expect(pick, "the Film Negative panel's Pick base button");
  const steps = t.historyLength();
  t.click(pick);
  await t.until("picking the base", () => ui.picking === "base" && $("viewer").classList.contains("picking"));
  const r = $("preview").getBoundingClientRect();
  const at = { clientX: r.left + r.width * 0.5, clientY: r.top + r.height * 0.5, button: 0, bubbles: true };
  $("preview").dispatchEvent(new MouseEvent("mousedown", at));
  window.dispatchEvent(new MouseEvent("mouseup", at));
  await t.until("a film base", () => t.setting("amb:FilmBaseRed") != null, 30000);
  t.expect(t.setting("amb:NegativeConvert") === true, "the conversion on");
  t.expect(!ui.picking && !pick.classList.contains("active"), "picking over");
  t.expect(t.historyLength() === steps + 1, "one history step");
  pick.dispatchEvent(new MouseEvent("click", { altKey: true, bubbles: true }));
  await t.until("the base measured again", () => t.setting("amb:FilmBaseRed") == null);
  t.key("Ctrl+Z");
  t.key("Ctrl+Z");
  await t.until("the photo as it was", () => !t.setting("amb:NegativeConvert"));
});

await t.step("Spot panel: Find dust finds none on a clean chart; Shift-drag heals a line as one step", async () => {
  t.key("Q");
  await t.until("the spot tool", () => sp.on && !$("spot-panel").hidden);
  const steps = t.historyLength();
  $("toast").hidden = true;
  t.click("#spot-dust");
  await t.until("its note", () => !$("toast").hidden && $("toast").textContent.includes("dust"), 60000);
  t.expect($("toast").textContent.startsWith("No dust found"), `the note: ${$("toast").textContent}`);
  t.expect(t.historyLength() === steps, "no history step");
  // Shift-drag heals along a straight line, as one step.
  const before = spotList().length;
  const c = $("spot-canvas").getBoundingClientRect();
  const pt = (fx, fy, extra = {}) => ({ clientX: c.left + c.width * fx, clientY: c.top + c.height * fy, button: 0, shiftKey: true, bubbles: true, pointerId: 1, ...extra });
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", pt(0.3, 0.5)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointermove", pt(0.5, 0.52)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", pt(0.7, 0.55)));
  await t.until("the scratch healed", () => spotList().length > before, 30000);
  t.expect(t.historyLength() === steps + 1, "one history step");
  t.key("Ctrl+Z");
  await t.until("undone", () => spotList().length === before);
  // Z and the 100% button zoom with the tool open, and the tool stays.
  t.expect(!$("btn-zoom").disabled, "the 100% button works in spot removal");
  t.key("z");
  await t.until("Z zooms in", () => ui.zoom.on);
  t.click("#btn-zoom");
  await t.until("and out", () => !ui.zoom.on);
  t.expect(sp.on, "still in spot removal");
  // Zoomed in, opening the tool keeps the zoom.
  exitSpots();
  setZoom(true);
  await t.until("zoomed", () => ui.zoom.on);
  t.key("Q");
  await t.until("the spot tool", () => sp.on);
  t.expect(ui.zoom.on, "the zoom kept");
  setZoom(false);
  await t.until("fit", () => !ui.zoom.on);
  // Ctrl + scroll zooms in at the pointer (not the spot size); arrows and a
  // right-drag move the zoomed-in photo with the tool open; Ctrl + scroll
  // back out.
  const size = sp.size;
  const wheelAt = (deltaY) => $("spot-canvas").dispatchEvent(new WheelEvent("wheel", {
    deltaY, ctrlKey: true, clientX: c.left + c.width / 2, clientY: c.top + c.height / 2, bubbles: true, cancelable: true }));
  wheelAt(-100);
  await t.until("zoomed in", () => ui.zoom.on && ui.zoom.meta);
  t.expect(sp.size === size, `the spot size stays (${sp.size})`);
  const cx = ui.zoom.cx;
  t.key("ArrowRight");
  await t.until("moved right", () => ui.zoom.cx > cx);
  const cy = ui.zoom.cy;
  const mouse = (type, x, y) => $("spot-canvas").dispatchEvent(new MouseEvent(type, { button: 2, clientX: x, clientY: y, bubbles: true }));
  mouse("mousedown", c.left + 300, c.top + 300);
  window.dispatchEvent(new MouseEvent("mousemove", { clientX: c.left + 300, clientY: c.top + 200 }));
  window.dispatchEvent(new MouseEvent("mouseup", { button: 2, clientX: c.left + 300, clientY: c.top + 200 }));
  await t.until("a right-drag moved it", () => ui.zoom.cy > cy);
  t.expect(sp.on, "still in spot removal");
  wheelAt(100);
  await t.until("the whole photo", () => !ui.zoom.on);
  // The Size slider sets the next spot's size as it moves, up to 25 %.
  const was = sp.size;
  $("spot-size").value = "20";
  $("spot-size").dispatchEvent(new Event("input"));
  t.expect(sp.size === 20 && $("spot-size").max === "25", `size ${sp.size}, max ${$("spot-size").max}`);
  $("spot-size").value = String(was);
  $("spot-size").dispatchEvent(new Event("input"));
  // Remove: a click makes a content-aware spot with no source.
  t.click('#spot-kind [data-kind="remove"]');
  t.expect(document.querySelector('#spot-kind [data-kind="remove"]').classList.contains("active"), "Remove chosen");
  const click = (extra) => pt(0.4, 0.4, { shiftKey: false, ...extra });
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", click()));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", click()));
  await t.until("the Remove spot", () => spotList().length > before, 30000);
  const r = spotList()[spotList().length - 1];
  t.expect(r.kind === "content-aware" && !r.source, `a Remove spot: ${JSON.stringify(r)}`);
  t.key("Ctrl+Z");
  await t.until("undone", () => spotList().length === before);
  t.click('#spot-kind [data-kind="heal"]');
  t.click("#spot-done");
  await t.until("the spot tool closed", () => !sp.on);
});

await t.step("Remove object: Shift+Q opens the tool; a box sends its corners, a click its point; Esc cancels", async () => {
  t.key("Shift+Q");
  await t.until("box mode", () => sp.on && sp.object && $("spot-object").classList.contains("active"));
  t.expect($("spot-hint").textContent.includes("box"), $("spot-hint").textContent);
  t.expect($("spot-panel").firstElementChild.nextElementSibling === $("spot-object"), "the panel's first button");
  // A drag draws a box and asks for that box (the model isn't run here).
  const spots = spotList().length;
  const asked = [];
  const real = window.removeObject;
  window.removeObject = async (target) => { asked.push(target); setObjectMode(false); };
  const c = $("spot-canvas").getBoundingClientRect();
  const pt = (fx, fy) => ({ clientX: c.left + c.width * fx, clientY: c.top + c.height * fy, button: 0, bubbles: true, pointerId: 1 });
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", pt(0.6, 0.6)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointermove", pt(0.4, 0.5)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", pt(0.3, 0.3)));
  window.removeObject = real;
  t.expect(asked.length === 1 && asked[0].rect, `asked ${JSON.stringify(asked)}`);
  const [x0, y0, x1, y1] = asked[0].rect;
  t.expect(x0 < x1 && y0 < y1, `corners in order: ${asked[0].rect}`);
  // A click asks for the object under it.
  setObjectMode(true);
  window.removeObject = async (target) => { asked.push(target); setObjectMode(false); };
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerdown", pt(0.5, 0.5)));
  $("spot-canvas").dispatchEvent(new PointerEvent("pointerup", pt(0.5, 0.5)));
  window.removeObject = real;
  t.expect(asked.length === 2 && asked[1].point?.length === 2, `a click: ${JSON.stringify(asked[1])}`);
  t.expect(!sp.object && spotList().length === spots, "one box, then back to spots; no spot drawn");
  // Esc leaves box mode first, then the tool.
  t.click("#spot-object");
  t.expect(sp.object, "the button turns it on");
  t.key("Escape");
  t.expect(!sp.object && sp.on, "Esc ends box mode, the tool stays");
  t.key("Escape");
  await t.until("the spot tool closed", () => !sp.on);
});

await t.step("tips stop after Not now three times, or at once with Don't show tips", async () => {
  await loadFeatures();
  const on = () => tipsOn("wait") && tipsOn("boxes");
  const reset = () => {
    for (const id of ["settings-tips-wait", "settings-tips-boxes"]) {
      $(id).checked = true;
      $(id).dispatchEvent(new Event("change"));
    }
  };
  reset();
  for (let i = 0; i < 3; i++) {
    t.expect(on(), `tips still on before Not now ${i + 1}`);
    t.expect(showTip(), "a tip to show");
    t.click("#tip-close");
    t.expect($("tip-card").hidden, "the card goes");
  }
  t.expect(!on() && !$("settings-tips-wait").checked && !$("settings-tips-boxes").checked, "off after three, in Settings too");
  t.expect($("toast").textContent.includes("Settings → General"), `says where: ${$("toast").textContent}`);
  reset();
  showTip();
  t.click("#tip-never");
  t.expect(!on() && $("tip-card").hidden, "Don't show tips: off at once");
  reset();
});

await t.step("Watch an edit on this photo: Auto edit's changes one History step at a time; Undo takes them back", async () => {
  const before = ui.state.history.length;
  const watching = watchOwnFlow();
  await t.until("the ghost cursor", () => document.querySelector(".ghost-cursor"), 20000);
  await watching;
  const added = ui.state.history.length - before;
  t.expect(added >= 2 && !document.querySelector(".ghost-cursor"), `${added} steps, then the ghost leaves`);
  await run(Array.from({ length: added }, () => ({ cmd: "undo" })));
  t.expect(ui.state.history.length === before, "back as it was");
});

await t.step("slow work shows a note on the photo with its seconds; a render after Remove says it's filling", async () => {
  const done = showWorking("Removing…", 0);
  await t.until("the note", () => !$("working-note").hidden);
  t.expect($("working-note").textContent.startsWith("Removing…"), $("working-note").textContent);
  done();
  t.expect($("working-note").hidden, "gone when the work is done");
  ui.preview.filling = true;
  requestPreview(false);
  await t.until("the render", () => !ui.preview.inFlight);
  t.expect(!ui.preview.filling && $("working-note").hidden, "the filling note ends with the render");
});

await t.step("a drag or Select All doesn't select the photo or the panels in blue", async () => {
  // The workspace: the photo, the panels and the toolbar.
  document.getSelection().selectAllChildren($("workspace"));
  const picked = String(document.getSelection()).trim();
  document.getSelection().removeAllRanges();
  t.expect(picked === "", `nothing selected (got ${picked.length} characters)`);
  t.expect(getComputedStyle(document.querySelector("textarea, input[type=text], input[type=search]")).userSelect !== "none", "text boxes still select");
});

await t.step("Appearance opens on every task (Trippy's plugin layout too)", async () => {
  const was = currentTask();
  for (const task of TASKS.map((x) => x.id).filter((id) => id !== "library")) {
    await switchTask(task);
    const flow = looksFlow();
    await t.until(`Appearance on ${task}`, () => $("looks-dialog").open);
    $("looks-dialog").close("cancel");
    await flow;
  }
  await switchTask(was);
});

await t.step("a narrow toolbar puts the buttons that don't fit under ⋯, and they still work", async () => {
  const bar = document.querySelector(".topbar .actions");
  // The top bar never runs past the window, whatever the window's width.
  const top = document.querySelector(".topbar").getBoundingClientRect();
  t.expect(top.right <= window.innerWidth + 1 && bar.getBoundingClientRect().right <= window.innerWidth, `top bar ${top.right} in a ${window.innerWidth} px window`);
  bar.style.maxWidth = "320px";
  fitToolbar();
  const hidden = overflowedButtons();
  t.expect(hidden.length > 3 && !$("btn-toolbar-more").hidden, `${hidden.length} under ⋯`);
  t.expect(bar.scrollWidth <= bar.clientWidth + 1, "nothing cut off at the edge");
  t.click("#btn-toolbar-more");
  const items = [...$("toolbar-menu").querySelectorAll("button")];
  t.expect(!$("toolbar-menu").hidden && items.length === hidden.length, `${items.length} in the menu`);
  let clicked = false;
  const last = hidden.at(-1);
  const spy = (e) => { clicked = true; e.stopImmediatePropagation(); };
  last.addEventListener("click", spy, { capture: true, once: true });
  items.at(-1).click();
  t.expect(clicked && $("toolbar-menu").hidden, `its entry clicks ${last.id} and closes the menu`);
  bar.style.maxWidth = "";
  fitToolbar();
  t.expect($("btn-toolbar-more").hidden || overflowedButtons().length < hidden.length, "room again: the buttons come back");
});

await t.step("Settings → AI models says where models run (GPU or CPU, and why)", async () => {
  await modelsTab();
  const said = $("models-runs-on").textContent;
  t.expect(/^Models run on the (GPU|CPU) \(.+\)/.test(said), `said: ${said}`);
});

await t.step("Feedback works without a GitHub account: by email, or copied", async () => {
  const flow = feedbackFlow();
  await t.until("the dialog", () => $("feedback-dialog").open);
  const buttons = [...$("feedback-dialog").querySelectorAll("menu button")].map((b) => b.value);
  t.expect(["copy", "email", "ok"].every((v) => buttons.includes(v)), buttons.join(", "));
  t.expect($("feedback-form").preview.value.includes("Kind"), "the report shown before sending");
  $("feedback-dialog").close("cancel");
  await flow;
});

await t.step("Ctrl+E exports a JPEG (quality 100, under 16 MB, with a smaller copy)", async () => {
  t.key("Ctrl+E");
  await t.until("the Export dialog", () => $("export-dialog").open);
  // Quality 100 by default; Keep under offers WhatsApp's 16 MB.
  const f = $("export-form");
  t.expect(f.quality.value === "100", `default quality ${f.quality.value}`);
  f.max_mb_pick.value = "16";
  f.max_mb_pick.dispatchEvent(new Event("change"));
  t.expect(f.max_mb.value === "16" && f.max_mb_pick.value === "", `kept under ${f.max_mb.value} MB`);
  t.expect(f.quality.closest("label").textContent.includes("at most"), "the quality reads as the most it gets");
  // And a smaller copy beside it.
  t.expect(f.small_mb.disabled, "its size waits for the switch");
  t.click('#export-form [name="small_copy"]');
  t.expect(!f.small_mb.disabled, "the size can be set");
  f.small_mb.value = "0.05";
  $("toast").hidden = true;
  t.click("#export-go");
  await t.until("the export", () => t.status().startsWith("Exported "), 120000);
  await t.until("its note", () => !$("toast").hidden);
  t.expect($("toast").textContent.includes("-small.jpg"), `the smaller copy: ${$("toast").textContent}`);
  t.click('#export-form [name="small_copy"]');
  f.max_mb.value = "";
  t.log(t.status());
  t.expect(lastExportDir() && t.status().includes(lastExportDir()), `the folder kept for next time: ${lastExportDir()}`);
});

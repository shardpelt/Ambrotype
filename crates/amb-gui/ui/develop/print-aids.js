// SPDX-License-Identifier: GPL-3.0-or-later
// Print aids: the test strip, the ring-around and the step wedge
// (amb_ops::darkroom). The photo is printed several ways on one sheet, shown
// in a dialog with each cell a button; clicking one applies its changes as
// one undo step. The wedge's cells are read, not clicked.
// Buttons in the Film Negative panel (next to darkroom.js's readouts) and
// key actions start them, and Pick Better (pick-better.js). Also here: placing a spot in a zone (right-
// click the photo; the exposure that prints it there, one undo step) and
// peeking at the negative (the scan as it is, not saved).
"use strict";

const aids = {
  kind: "",          // "strip", "ring" or "wedge", the one on screen
  strip: null, ring: null, wedge: null, // the panel's buttons (made with the panel)
  seq: 0,            // the newest sheet asked for; older answers are dropped
  peek: false, peekBtn: null, // peeking at the negative, and the panel's button
  pop: null,         // the zone picker, while it is open
};

// The panel's two buttons, in a row under darkroom.js's readouts.
function aidsButtons() {
  aids.strip = el("button", { type: "button", id: "aids-strip", title: t("Print the photo in bands, each a step lighter or darker") }, t("Test strip…"));
  aids.ring = el("button", { type: "button", id: "aids-ring" }, t("Ring-around…"));
  aids.wedge = el("button", { type: "button", id: "aids-wedge" }, t("Step wedge…"));
  aids.wedge.addEventListener("click", () => aidsOpen("wedge"));
  const better = el("button", { type: "button", id: "aids-pick", title: t("Two prints side by side: click the better one, a setting at a time") }, t("Pick Better…"));
  aids.strip.addEventListener("click", () => aidsOpen("strip"));
  better.addEventListener("click", () => pickOpen());
  aids.ring.addEventListener("click", () => aidsOpen("ring"));
  aids.peekBtn = el("button", { type: "button", id: "aids-peek", title: t("Show the scan as it is, without the conversion (Shift+N). Not saved.") }, t("Peek"));
  aids.peekBtn.addEventListener("click", () => aidsPeekToggle());
  aidsSync();
  return el("div", { class: "aids-buttons" }, aids.strip, better, aids.ring, aids.wedge, aids.peekBtn);
}

// The ring-around and the step wedge need the conversion on.
function aidsSync() {
  if (!aids.ring?.isConnected) return;
  // The conversion went off (undo, say) under a peek: back to normal.
  if (aids.peek && !asBool(currentValue("amb:NegativeConvert"))) aidsPeekSet(false);
  aids.peekBtn.classList.toggle("active", aids.peek);
  const off = !ui.info || !asBool(currentValue("amb:NegativeConvert"));
  aids.ring.disabled = off;
  aids.ring.title = off
    ? t("Ring-around works on a converted negative: switch on Film Negative first.")
    : t("Print the photo nine ways: a step more of each colour, lighter and darker");
  aids.wedge.disabled = off;
  aids.wedge.title = off
    ? t("The step wedge works on a converted negative: switch on Film Negative first.")
    : t("Print a 21-step wedge as this negative prints: which steps merge into white or black");
}

async function aidsOpen(kind) {
  if (!ui.info) return toast(t("Open a photo first."), true);
  if (kind === "ring" && !asBool(currentValue("amb:NegativeConvert"))) {
    return toast(t("Ring-around works on a converted negative: switch on Film Negative first."), true);
  }
  if (kind === "wedge" && !asBool(currentValue("amb:NegativeConvert"))) {
    return toast(t("The step wedge works on a converted negative: switch on Film Negative first."), true);
  }
  aids.kind = kind;
  const seq = ++aids.seq;
  $("aids-title").textContent = aidsName(kind);
  $("aids-hint").textContent = kind === "wedge"
    ? t("Each step is 0.15 denser than the last, printed as this negative prints. White and black mark the steps that merge. Esc closes.")
    : t("Click a print to use it. Esc closes.");
  $("aids-stage").hidden = true;
  $("aids-hint").hidden = true;
  $("aids-status").textContent = t("Printing…");
  if (!$("aids-dialog").open) $("aids-dialog").showModal();
  const button = { ring: aids.ring, wedge: aids.wedge }[kind] ?? aids.strip;
  const ok = await withBusy(button ?? $("aids-close"), $("aids-status"), async () => {
    const sheet = await invoke("darkroom_aid", { kind });
    if (seq !== aids.seq || !$("aids-dialog").open) return;
    aidsShow(sheet);
  });
  if (!ok) $("aids-stage").hidden = true;
}

// Draw the sheet with a button over each cell.
function aidsShow({ image, aid }) {
  const stage = $("aids-stage");
  const img = $("aids-img");
  img.src = image;
  const read = aids.kind === "wedge";
  const boxes = aid.cells.map((c) => {
    const box = read
      ? el("div", { class: "aids-cell aids-read" }, el("span", {}, c.label))
      : el("button", { type: "button", class: "aids-cell", title: aidsTitle(c) }, el("span", {}, t(c.label)));
    box.style.setProperty("--x", `${(c.x / aid.width) * 100}%`);
    box.style.setProperty("--y", `${(c.y / aid.height) * 100}%`);
    box.style.setProperty("--w", `${(c.w / aid.width) * 100}%`);
    box.style.setProperty("--h", `${(c.h / aid.height) * 100}%`);
    if (!read) box.addEventListener("click", () => aidsApply(c));
    return box;
  });
  stage.replaceChildren(img, ...boxes);
  stage.hidden = false;
  $("aids-hint").hidden = false;
  $("aids-status").textContent = "";
}

function aidsName(kind) {
  return { ring: t("Ring-around"), wedge: t("Step wedge") }[kind] ?? t("Test strip");
}

function aidsTitle(c) {
  const names = Object.keys(c.changes);
  return names.length ? t("Print it like this") : t("Keep it as it is");
}

// One undo step for all the cell's settings; "as now" changes nothing.
async function aidsApply(cell) {
  $("aids-dialog").close();
  const commands = Object.entries(cell.changes).map(([name, value]) => ({ cmd: "set_param", name, value }));
  if (!commands.length) return;
  await run(commands, `${aidsName(aids.kind)} ${t(cell.label)}`);
}

// ---------------------------------------------------------------- place a spot in a zone

// The photo's right-click items: place the spot under the pointer in a zone.
function aidsPlaceItems(e) {
  const at = ui.info && darkroomWhole() ? darkroomAt(e) : null;
  return at ? [[t("Place this spot in a zone…"), () => aidsZonePop(e, at)], null] : [];
}

function aidsZoneClose() {
  aids.pop?.remove();
  aids.pop = null;
}

// A row of the zones 0 to X in their overlay colours, at the click.
function aidsZonePop(e, at) {
  aidsZoneClose();
  const swatches = ZONE_COLOURS.map((colour, zone) => {
    const b = el("button", { type: "button", class: "zone-swatch", title: t("Zone {zone}", { zone: DARKROOM_ZONES[zone] }) }, DARKROOM_ZONES[zone]);
    b.style.setProperty("--zone", colour);
    b.addEventListener("click", () => {
      aidsZoneClose();
      aidsPlace(at, zone).catch(showError);
    });
    return b;
  });
  aids.pop = el("div", { class: "zone-pop", role: "dialog", "aria-label": t("Place this spot in a zone") },
    el("div", { class: "zone-pop-title" }, t("Place this spot in zone…")),
    el("div", { class: "zone-pop-row" }, ...swatches));
  document.body.append(aids.pop);
  const r = aids.pop.getBoundingClientRect();
  aids.pop.style.left = `${Math.max(4, Math.min(e.clientX, innerWidth - r.width - 4))}px`;
  aids.pop.style.top = `${Math.max(4, Math.min(e.clientY, innerHeight - r.height - 4))}px`;
}

// Ask for the exposure that puts the spot in the zone, then set it: one undo step.
async function aidsPlace(at, zone) {
  if (!ui.info) return;
  status(t("Placing the spot…"), true);
  $("busy").hidden = false;
  let placed;
  try {
    placed = await invoke("darkroom_place", { at, zone });
  } finally {
    $("busy").hidden = true;
    status(t("Ready"));
  }
  const name = DARKROOM_ZONES[zone];
  const commands = Object.entries(placed.changes).map(([setting, value]) => ({ cmd: "set_param", name: setting, value }));
  if (!commands.length) return toast(t("The spot is in zone {zone} already.", { zone: DARKROOM_ZONES[Math.round(placed.zone_before)] ?? name }));
  await run(commands, t("Place in zone {zone}", { zone: name }));
  if (!placed.reached) toast(t("The exposure stops at the end of its range, short of zone {zone}.", { zone: name }), true);
}

// ---------------------------------------------------------------- peek at the negative

// The preview shows the scan as it is while on; the session keeps the flag,
// nothing is saved and there is no undo step.
async function aidsPeekSet(on) {
  aids.peek = on;
  aids.peekBtn?.classList.toggle("active", on);
  try {
    await invoke("set_peek_negative", { on });
  } catch (e) {
    aids.peek = false;
    aids.peekBtn?.classList.remove("active");
    return showError(e);
  }
  status(on ? t("Peeking at the negative: Shift+N goes back") : t("Ready"), true);
  requestPreview(false);
}

function aidsPeekToggle() {
  if (!ui.info) return toast(t("Open a photo first."), true);
  if (!aids.peek && !asBool(currentValue("amb:NegativeConvert"))) {
    return toast(t("Peek works on a converted negative: switch on Film Negative first."), true);
  }
  return aidsPeekSet(!aids.peek);
}

// Another photo opened: a new session, so the peek is off there.
function aidsPhotoChanged() {
  aids.peek = false;
  aids.peekBtn?.classList.remove("active");
}

function wirePrintAids() {
  addEventListener("pointerdown", (e) => { if (aids.pop && !aids.pop.contains(e.target)) aidsZoneClose(); }, true);
  addEventListener("keydown", (e) => { if (aids.pop && e.key === "Escape") { e.stopPropagation(); aidsZoneClose(); } }, true);
  $("aids-close").addEventListener("click", () => $("aids-dialog").close());
  $("aids-dialog").addEventListener("close", () => { aids.seq++; });
  tauri.event.listen("darkroom-progress", (e) => {
    const [done, total] = e.payload;
    if ($("aids-dialog").open && total) $("aids-status").textContent = t("Printing {done} of {total}…", { done, total });
  });
}

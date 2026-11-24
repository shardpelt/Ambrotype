// SPDX-License-Identifier: GPL-3.0-or-later
// Print aids: the test strip and the ring-around (amb_ops::darkroom). The
// photo is printed several ways on one sheet, shown in a dialog with each
// cell a button; clicking one applies its changes as one undo step.
// Two buttons in the Film Negative panel (next to darkroom.js's readouts)
// and two key actions start them.
"use strict";

const aids = {
  kind: "",          // "strip" or "ring", the one on screen
  strip: null, ring: null, // the panel's buttons (made with the panel)
  seq: 0,            // the newest sheet asked for; older answers are dropped
};

// The panel's two buttons, in a row under darkroom.js's readouts.
function aidsButtons() {
  aids.strip = el("button", { type: "button", id: "aids-strip", title: t("Print the photo in bands, each a step lighter or darker") }, t("Test strip…"));
  aids.ring = el("button", { type: "button", id: "aids-ring" }, t("Ring-around…"));
  aids.strip.addEventListener("click", () => aidsOpen("strip"));
  aids.ring.addEventListener("click", () => aidsOpen("ring"));
  aidsSync();
  return el("div", { class: "aids-buttons" }, aids.strip, aids.ring);
}

// The ring-around needs the conversion on.
function aidsSync() {
  if (!aids.ring?.isConnected) return;
  const off = !ui.info || !asBool(currentValue("amb:NegativeConvert"));
  aids.ring.disabled = off;
  aids.ring.title = off
    ? t("Ring-around works on a converted negative: switch on Film Negative first.")
    : t("Print the photo nine ways: a step more of each colour, lighter and darker");
}

async function aidsOpen(kind) {
  if (!ui.info) return toast(t("Open a photo first."), true);
  if (kind === "ring" && !asBool(currentValue("amb:NegativeConvert"))) {
    return toast(t("Ring-around works on a converted negative: switch on Film Negative first."), true);
  }
  aids.kind = kind;
  const seq = ++aids.seq;
  $("aids-title").textContent = kind === "ring" ? t("Ring-around") : t("Test strip");
  $("aids-stage").hidden = true;
  $("aids-hint").hidden = true;
  $("aids-status").textContent = t("Printing…");
  if (!$("aids-dialog").open) $("aids-dialog").showModal();
  const button = kind === "ring" ? aids.ring : aids.strip;
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
  const boxes = aid.cells.map((c) => {
    const box = el("button", { type: "button", class: "aids-cell", title: aidsTitle(c) }, el("span", {}, t(c.label)));
    box.style.setProperty("--x", `${(c.x / aid.width) * 100}%`);
    box.style.setProperty("--y", `${(c.y / aid.height) * 100}%`);
    box.style.setProperty("--w", `${(c.w / aid.width) * 100}%`);
    box.style.setProperty("--h", `${(c.h / aid.height) * 100}%`);
    box.addEventListener("click", () => aidsApply(c));
    return box;
  });
  stage.replaceChildren(img, ...boxes);
  stage.hidden = false;
  $("aids-hint").hidden = false;
  $("aids-status").textContent = "";
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
  await run(commands, `${aids.kind === "ring" ? t("Ring-around") : t("Test strip")} ${t(cell.label)}`);
}

function wirePrintAids() {
  $("aids-close").addEventListener("click", () => $("aids-dialog").close());
  $("aids-dialog").addEventListener("close", () => { aids.seq++; });
  tauri.event.listen("darkroom-progress", (e) => {
    const [done, total] = e.payload;
    if ($("aids-dialog").open && total) $("aids-status").textContent = t("Printing {done} of {total}…", { done, total });
  });
}

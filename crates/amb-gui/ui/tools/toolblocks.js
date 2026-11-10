// SPDX-License-Identifier: GPL-3.0-or-later
// Tool blocks: big buttons for a task's own tools, first in its layout, so
// each task shows what it is for. Develop's Quick fixes (one-click fixes
// for the whole photo) and Retouch's tools (Remove object, spots, dust and
// the AI masks). Each button says what it does under its name; they only
// start existing tools.
"use strict";

// Level the horizon (cropped to fit), as the crop tool's Level, one step.
async function levelFlow() {
  if (!ui.info) return toast("Open a photo first.", true);
  if (ui.crop.on) await exitCrop(true);
  try {
    const a = await invoke("level_angle");
    if (a == null) return toast("No clear horizon or straight lines to level by: try Crop (R) and turn it by hand.", true);
    if (Math.abs(a) < 0.05) return toast("Already level.");
    const st = await invoke("crop_state");
    const rect = AmbCrop.straighten(st.rect, Math.max(-45, Math.min(45, a)), ui.info.width, ui.info.height, st.angle_sign);
    await run(await invoke("crop_commands", { rect }), "Level");
    toast(`Levelled by ${a > 0 ? "+" : ""}${a.toFixed(1)}°.`);
  } catch (e) {
    toast(String(e), true);
  }
}

// A new AI or range mask of `kind`, opening Masks for it.
async function maskFlow(kind) {
  if (!ui.info) return toast("Open a photo first.", true);
  await enterMasks();
  if (mk.on) startTool(kind, true);
}

async function spotFlow(kind) {
  if (!ui.info) return toast("Open a photo first.", true);
  await enterSpots();
  if (sp.on) setSpotKind(kind);
}

const TOOL_BLOCKS = {
  quick: [
    { id: "auto", label: "Auto edit", big: true, hint: "Tone, white balance, lens and a level horizon", run: () => autoEditFlow() },
    { id: "tone", label: "Auto tone", hint: "Exposure, contrast, highlights, shadows", run: () => autoFlow(true, false) },
    { id: "wb", label: "Auto white balance", hint: "Takes out a colour cast", run: () => autoFlow(false, true) },
    { id: "level", label: "Level", hint: "Straightens a crooked horizon", run: () => levelFlow() },
    {
      id: "lens", label: "Lens fixes", hint: "Profile corrections and colour fringes",
      run: () => run([
        { cmd: "set_param", name: "LensProfileEnable", value: true },
        { cmd: "set_param", name: "AutoLateralCA", value: true },
      ], "Lens fixes"),
    },
    { id: "surprise", label: "Surprise me", hint: "Six random looks to pick from", run: () => surpriseFlow() },
  ],
  retouch: [
    { id: "select", label: "Select object", big: true, hint: "Click it: adjust, remove, move it", run: () => enterSelect() },
    { id: "object", label: "Remove object", hint: "Click it or box it; AI removes all of it", run: async () => { await enterSpots(); if (sp.on) setObjectMode(true); } },
    { id: "remove", label: "AI Remove", hint: "Click or brush over something", run: () => spotFlow("remove") },
    { id: "heal", label: "Heal", hint: "Blemishes and specks", run: () => spotFlow("heal") },
    { id: "dust", label: "Find dust", hint: "Specks and hairs on a scan", run: async () => { await enterSpots(); if (sp.on) findDust(); } },
    { id: "subject", label: "Select subject", hint: "AI mask of the main subject", run: () => maskFlow("subject") },
    { id: "people", label: "Select people", hint: "AI mask of everyone", run: () => maskFlow("people") },
    { id: "background", label: "Select background", hint: "Everything but the subject", run: () => maskFlow("background") },
    { id: "sky", label: "Select sky", hint: "The sky, joined to the top", run: () => maskFlow("sky") },
    { id: "brush", label: "Brush", hint: "Paint where to change", run: () => maskFlow("brush") },
  ],
};

function wireToolBlocks() {
  for (const [name, tools] of Object.entries(TOOL_BLOCKS)) {
    const box = $(`tools-${name}`);
    if (!box) continue;
    box.replaceChildren(...tools.map((t) => {
      const b = Object.assign(document.createElement("button"), { type: "button", className: `tool-tile${t.big ? " big" : ""}`, title: t.hint });
      b.dataset.tool = t.id;
      b.append(Object.assign(document.createElement("strong"), { textContent: t.label }), Object.assign(document.createElement("span"), { textContent: t.hint }));
      b.addEventListener("click", () => {
        if (!ui.info) return toast("Open a photo first.", true);
        Promise.resolve(t.run()).catch((e) => toast(String(e), true));
      });
      return b;
    }));
  }
}

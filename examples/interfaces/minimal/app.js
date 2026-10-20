// SPDX-License-Identifier: GPL-3.0-or-later
// A whole interface in one file: it talks to the app through the same
// commands as the built-in one (docs/ai/custom-interface.md).
"use strict";

const { invoke } = window.__TAURI__.core;
const $ = (id) => document.getElementById(id);

// The settings this interface shows; their labels and ranges come from the app.
const SHOWN = ["Exposure2012", "Contrast2012", "Highlights2012", "Shadows2012", "Temperature", "Tint", "Vibrance", "Saturation"];
const inputs = new Map();
let pending = null;
// This photo's own defaults (as-shot white balance and the like).
let defaults = {};

async function buildSliders() {
  const panels = await invoke("editor_schema");
  const controls = panels.flatMap((p) => p.sections.flatMap((s) => s.controls));
  for (const name of SHOWN) {
    const c = controls.find((x) => x.name === name);
    if (!c || c.kind !== "number") continue;
    const input = Object.assign(document.createElement("input"), { type: "range", min: c.min, max: c.max, step: c.step, value: c.default ?? 0, disabled: true });
    const value = document.createElement("b");
    const label = document.createElement("label");
    label.append(Object.assign(document.createElement("span"), { textContent: c.label }), input);
    label.firstChild.append(value);
    input.addEventListener("input", () => { value.textContent = input.value; });
    input.addEventListener("change", () => edit([{ cmd: "set_param", name, value: Number(input.value) }]));
    input.addEventListener("dblclick", () => edit([{ cmd: "set_param", name, value: defaults[name] ?? c.default ?? 0 }]));
    inputs.set(name, { input, value, fallback: c.default });
    $("sliders").append(label);
  }
}

function showState(state) {
  for (const [name, { input, value, fallback }] of inputs) {
    const v = state.settings[name] ?? defaults[name] ?? fallback;
    if (typeof v === "number") input.value = v;
    value.textContent = input.value;
    input.disabled = false;
  }
  $("undo").disabled = !state.can_undo;
  $("redo").disabled = !state.can_redo;
  $("save").disabled = false;
}

// The preview comes framed: a u32 length, that much JSON, then the JPEG.
async function render() {
  if (pending) return void (pending.again = true);
  pending = { again: false };
  try {
    const edge = Math.round(Math.max($("view").clientWidth, $("view").clientHeight) * devicePixelRatio);
    const buf = new Uint8Array(await invoke("preview", { maxEdge: edge, overrides: {}, before: false, clipping: false, maskOverlay: null }));
    const len = new DataView(buf.buffer, buf.byteOffset, 4).getUint32(0, true);
    const meta = JSON.parse(new TextDecoder().decode(buf.subarray(4, 4 + len)));
    const old = $("photo").src;
    $("photo").src = URL.createObjectURL(new Blob([buf.subarray(4 + len)], { type: "image/jpeg" }));
    if (old) URL.revokeObjectURL(old);
    $("status").textContent = `${meta.width}×${meta.height} in ${meta.millis} ms`;
  } catch (e) {
    $("status").textContent = String(e);
  }
  const again = pending.again;
  pending = null;
  if (again) render();
}

async function edit(commands) {
  try {
    showState((await invoke("execute", { commands })).state);
    render();
  } catch (e) {
    $("status").textContent = String(e);
  }
}

async function open(path) {
  if (!path) return;
  try {
    const opened = await invoke("open_photo", { path });
    $("empty").hidden = true;
    defaults = { Temperature: opened.info.as_shot_temperature, Tint: opened.info.as_shot_tint, ...opened.info.defaults };
    document.title = `${opened.info.name} — Ambrotype`;
    showState(opened.state);
    render();
  } catch (e) {
    $("status").textContent = String(e);
  }
}

$("open").addEventListener("click", async () => open(await invoke("pick_photo")));
$("undo").addEventListener("click", () => edit([{ cmd: "undo" }]));
$("redo").addEventListener("click", () => edit([{ cmd: "redo" }]));
$("save").addEventListener("click", async () => {
  try {
    $("status").textContent = `Saved to ${await invoke("save_sidecar")}`;
  } catch (e) {
    $("status").textContent = String(e);
  }
});
// Always give people a way back.
$("back").addEventListener("click", () => invoke("set_interface", { path: null }));
window.addEventListener("resize", () => { if (!$("photo").hidden && $("photo").src) render(); });

buildSliders().then(async () => open(await invoke("take_initial_path")));

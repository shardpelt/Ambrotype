// SPDX-License-Identifier: GPL-3.0-or-later
// Effect plugins in the Effects panel (docs/ai/plugins.md): the effects on
// this photo (amb:Effects, a JSON list), each with an amount and the
// controls its manifest asks for, and a menu to add one. Changes go
// through set_param like any other edit (one undo step each); while a
// slider moves, the preview shows it through an override.
"use strict";

const fx = { plugins: [] };

async function loadEffectPlugins() {
  try {
    const all = await invoke("plugins_list");
    fx.plugins = all.filter((p) => p.manifest?.kind === "effect");
  } catch {
    fx.plugins = [];
  }
}

function effectsOf(v) {
  try {
    const list = JSON.parse(typeof v === "string" && v ? v : "[]");
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

// Where an effect can be kept to (amb:Effects `where`, the engine's
// plugin_effects::Place), and how much of it a colour (sRGB 0–1) gets.
const FX_PLACES = ["Highlights", "Midtones", "Shadows", "Reds", "Oranges", "Yellows", "Greens", "Aquas", "Blues", "Purples", "Magentas"];
const FX_HUES = { reds: 0, oranges: 30, yellows: 60, greens: 120, aquas: 180, blues: 225, purples: 270, magentas: 315 };

function placeWeight(place, [r, g, b]) {
  const smooth = (a, c, x) => {
    const t = Math.min(1, Math.max(0, (x - a) / (c - a)));
    return t * t * (3 - 2 * t);
  };
  const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  const p = place.toLowerCase();
  if (p === "highlights") return smooth(0.45, 0.8, l);
  if (p === "shadows") return 1 - smooth(0.2, 0.55, l);
  if (p === "midtones") return Math.max(0, 1 - ((l - 0.5) / 0.32) ** 2);
  const centre = FX_HUES[p];
  if (centre === undefined) return 1;
  const hi = Math.max(r, g, b);
  const chroma = hi - Math.min(r, g, b);
  if (chroma <= 1e-4) return 0;
  const h = hi === r ? 60 * ((((g - b) / chroma) % 6) + 6) % 360 : hi === g ? 60 * ((b - r) / chroma + 2) : 60 * ((r - g) / chroma + 4);
  let d = ((h - centre) % 360 + 360) % 360;
  d = Math.min(d, 360 - d);
  return Math.max(0, 1 - d / 40) * smooth(0.04, 0.18, chroma);
}

// Hovering an effect's Where: where it goes, tinted on the photo (judged
// from the preview as it is, which is close enough to show the place).
function showPlace(place) {
  const c = $("where-canvas");
  const img = $("preview");
  if (!place || !img.naturalWidth || !ui.info) {
    c.hidden = true;
    return;
  }
  c.hidden = false;
  const dpr = window.devicePixelRatio || 1;
  c.width = Math.round(c.clientWidth * dpr);
  c.height = Math.round(c.clientHeight * dpr);
  const r = viewRect("where-canvas");
  const [x0, y0] = [Math.round(r.left * dpr), Math.round(r.top * dpr)];
  const [w, h] = [Math.max(1, Math.round(r.width * dpr)), Math.max(1, Math.round(r.height * dpr))];
  const off = new OffscreenCanvas(w, h);
  const og = off.getContext("2d", { willReadFrequently: true });
  og.drawImage(img, 0, 0, w, h);
  const px = og.getImageData(0, 0, w, h);
  const d = px.data;
  for (let i = 0; i < d.length; i += 4) {
    const k = placeWeight(place, [d[i] / 255, d[i + 1] / 255, d[i + 2] / 255]);
    // Red where it goes (as much as it does), the rest darkened.
    const t = 0.6 * k;
    const dim = 0.45 + 0.55 * Math.min(1, k * 4);
    d[i] = (d[i] * (1 - t) + 255 * t) * dim;
    d[i + 1] = d[i + 1] * (1 - t) * dim;
    d[i + 2] = d[i + 2] * (1 - t) * dim;
  }
  const g = c.getContext("2d");
  g.clearRect(0, 0, c.width, c.height);
  g.putImageData(px, x0, y0);
}

function makeEffectsControl(def) {
  const box = document.createElement("div");
  box.className = "fx";
  let list = [];
  const preview = () => {
    ui.overrides["amb:Effects"] = JSON.stringify(list);
    requestPreview(true);
  };
  const save = (what) => {
    delete ui.overrides["amb:Effects"];
    const value = JSON.stringify(list);
    run(list.length
      ? [{ cmd: "set_param", name: "amb:Effects", value }]
      : [{ cmd: "reset_param", name: "amb:Effects" }], what);
  };
  // One parameter's control, from the plugin's manifest.
  const param = (e, p, name) => {
    const row = document.createElement("div");
    row.className = "row";
    const label = Object.assign(document.createElement("label"), { textContent: p.label, title: p.description ?? "" });
    const value = e.params?.[p.name] ?? p.default;
    let input;
    if (p.type === "number") {
      input = Object.assign(document.createElement("input"), { type: "range", min: p.min, max: p.max, step: p.step ?? (p.max - p.min) / 100, value });
      const out = Object.assign(document.createElement("output"), { value: Math.round(value * 100) / 100 });
      input.addEventListener("input", () => {
        e.params = { ...e.params, [p.name]: Number(input.value) };
        out.value = input.value;
        preview();
      });
      input.addEventListener("change", () => save(`${name}: ${p.label}`));
      row.append(label, input, out);
      return row;
    }
    if (p.type === "choice") {
      input = document.createElement("select");
      for (const o of p.options) input.append(new Option(o, o));
      input.value = value;
    } else {
      input = Object.assign(document.createElement("input"), { type: "checkbox", checked: Boolean(value) });
    }
    input.addEventListener("change", () => {
      e.params = { ...e.params, [p.name]: p.type === "bool" ? input.checked : input.value };
      save(`${name}: ${p.label}`);
    });
    row.append(label, input);
    return row;
  };
  const render = () => {
    const parts = list.map((e, i) => {
      const plugin = fx.plugins.find((p) => p.id === e.plugin);
      const m = plugin?.manifest;
      const name = m?.name ?? e.plugin;
      const card = document.createElement("div");
      card.className = "fx-effect";
      const head = document.createElement("div");
      head.className = "fx-head";
      const title = Object.assign(document.createElement("strong"), { textContent: name, title: m?.description ?? "" });
      // In Trippy, Amount goes past 100: the change pushed further than
      // the effect makes it.
      const wild = currentTask() === "trippy";
      const amount = Object.assign(document.createElement("input"), { type: "range", min: 0, max: wild ? 200 : 100, step: 1, value: e.amount ?? 100, title: wild ? "Amount (past 100: pushed further)" : "Amount" });
      amount.addEventListener("input", () => { e.amount = Number(amount.value); preview(); });
      amount.addEventListener("change", () => save(`${name}: amount`));
      const up = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "↑", title: "Apply earlier", disabled: i === 0 });
      up.addEventListener("click", () => { [list[i - 1], list[i]] = [list[i], list[i - 1]]; save(`move ${name}`); });
      const remove = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "×", title: `Remove ${name}` });
      remove.addEventListener("click", () => { list.splice(i, 1); save(`remove ${name}`); });
      head.append(title, amount, up, remove);
      card.append(head);
      if (!m) {
        card.append(Object.assign(document.createElement("p"), { className: "hint", textContent: "This plugin isn't installed here, so the effect isn't shown. Install it to see it, or remove it." }));
      } else {
        // Where: everywhere, or only the highlights, a colour…
        const where = document.createElement("div");
        where.className = "row fx-where";
        const pick = document.createElement("select");
        pick.append(new Option("Everywhere", ""), ...FX_PLACES.map((x) => new Option(x, x.toLowerCase())));
        pick.value = e.where ?? "";
        pick.title = "Only on part of the photo: hover to see where";
        pick.addEventListener("change", () => {
          if (pick.value) e.where = pick.value;
          else delete e.where;
          showPlace(pick.value);
          save(`${name}: where`);
        });
        const show = () => showPlace(pick.value);
        const hide = () => showPlace(null);
        where.addEventListener("mouseenter", show);
        where.addEventListener("mouseleave", hide);
        pick.addEventListener("focus", show);
        pick.addEventListener("blur", hide);
        where.append(Object.assign(document.createElement("label"), { textContent: "Where" }), pick);
        card.append(where);
        for (const p of m.params ?? []) card.append(param(e, p, name));
      }
      return card;
    });
    const add = document.createElement("select");
    add.className = "fx-add";
    add.append(new Option(fx.plugins.length ? "Add an effect…" : "No effect plugins installed", ""));
    for (const p of fx.plugins) add.append(new Option(p.manifest.name, p.id));
    add.disabled = !fx.plugins.length;
    add.title = "Effects from plugins, rendered with the photo after the colour and curves (Plugins → Browse hub… has more)";
    add.addEventListener("change", () => {
      const id = add.value;
      if (!id) return;
      // A seed of its own, so its randomness stays put for this photo.
      list.push({ plugin: id, params: {}, amount: 100, seed: Math.floor(Math.random() * 2 ** 31) });
      save(`add ${fx.plugins.find((p) => p.id === id)?.manifest.name ?? id}`);
    });
    box.replaceChildren(...parts, add);
  };
  const entry = {
    def,
    set(v) {
      list = effectsOf(v);
      render();
    },
  };
  ui.controls.set(def.name, entry);
  // The installed effect plugins can change (Plugins dialog): ask again.
  loadEffectPlugins().then(() => entry.set(currentValue(def.name)));
  render();
  return box;
}

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
      const amount = Object.assign(document.createElement("input"), { type: "range", min: 0, max: 100, step: 1, value: e.amount ?? 100, title: "Amount" });
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

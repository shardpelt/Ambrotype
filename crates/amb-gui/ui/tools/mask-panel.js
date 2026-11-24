// SPDX-License-Identifier: GPL-3.0-or-later
// Masks: the Masking panel and the drafts (masks being made). Split out of
// masks.js.
"use strict";

// ---------------------------------------------------------------- panel

// The label and range of one of the panel's own sliders, from the protocol's
// table (mask_field_ranges), so the panel and the commands' clamps agree.
function maskRange(key) {
  const r = mk.ranges.find((x) => x.key === key);
  return { label: t(r.label), min: r.min, max: r.max, step: r.step };
}
const maskDefault = (key) => mk.ranges.find((x) => x.key === key).default;

// How a part joins the ones above it: the command's word and its label.
const MASK_MODES = [["add", tKey("Add")], ["subtract", tKey("Subtract")], ["intersect", tKey("Intersect")]];

function sliderRow({ label, min, max, step, value, title, onInput, onChange, disabled }) {
  const decimals = step < 1 ? 2 : 0;
  const show = (v) => (min < 0 && v > 0 ? "+" : "") + Number(v).toFixed(decimals);
  const range = el("input", { type: "range", min, max, step, value, disabled });
  const num = el("input", { class: "value", type: "text", inputmode: "decimal", spellcheck: "false", value: show(value), disabled });
  const name = el("label", { title }, label);
  if (Number(value) !== 0 && min < 0) name.classList.add("changed");
  range.addEventListener("input", () => {
    num.value = show(range.value);
    onInput?.(Number(range.value));
  });
  range.addEventListener("change", () => onChange(Number(range.value)));
  range.addEventListener("dblclick", () => {
    const d = min < 0 ? 0 : Number(range.defaultValue ?? min);
    range.value = d;
    num.value = show(d);
    onChange(d);
  });
  num.addEventListener("change", () => {
    const v = Math.min(max, Math.max(min, Number(num.value.replace(",", "."))));
    if (!Number.isFinite(v)) return;
    range.value = v;
    num.value = show(v);
    onChange(v);
  });
  return el("div", { class: "row mask-slider" }, name, range, num);
}

function renderMaskPanel() {
  if (!mk.on) return;
  const list = maskList();
  if (mk.selected != null && mk.selected >= list.length) {
    mk.selected = list.length ? list.length - 1 : null;
    mk.part = null;
  }
  const m = selectedMask();
  if (m && mk.part != null && mk.part >= m.parts.length) mk.part = null;

  // New-mask buttons.
  const newRow = $("mask-new");
  newRow.replaceChildren(...NEW_KINDS.map((k) => {
    const active = mk.tool === k.kind && mk.creatingNew;
    return el("button", {
      type: "button",
      "data-ai": aiFreeUses(k.kind),
      class: "small" + (active ? " active" : ""),
      title: k.key
        ? t("New {kind} mask ({key}). {hint}", { kind: t(k.label).toLowerCase(), key: k.key, hint: t(k.hint) })
        : t("New {kind} mask. {hint}", { kind: t(k.label).toLowerCase(), hint: t(k.hint) }),
      onclick: () => startTool(k.kind, true),
    }, t(k.label));
  }));

  // The mask list.
  const rows = list.map((mask) => {
    const sel = mask.index === mk.selected;
    const eye = el("input", { type: "checkbox", title: t("Show this mask's effect"), checked: mask.active });
    eye.addEventListener("change", () => run([{ cmd: "set_mask", mask: mask.index, values: { active: eye.checked } }]));
    const name = el("span", { class: "mask-name", title: t("Double-click to rename"), translate: "no" }, mask.name);
    name.addEventListener("dblclick", (e) => {
      e.stopPropagation();
      const input = el("input", { type: "text", value: mask.name, spellcheck: "false" });
      name.replaceWith(input);
      input.focus();
      input.select();
      const done = (save) => {
        if (save && input.value.trim() && input.value !== mask.name) {
          run([{ cmd: "set_mask", mask: mask.index, values: { name: input.value.trim() } }]);
        } else renderMaskPanel();
      };
      input.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") done(true);
        if (ev.key === "Escape") done(false);
        ev.stopPropagation();
      });
      input.addEventListener("blur", () => done(true));
    });
    const del = el("button", { type: "button", class: "small icon-x", title: t("Delete this mask") }, "×");
    del.addEventListener("click", (e) => {
      e.stopPropagation();
      run([{ cmd: "remove_mask", mask: mask.index }]);
    });
    const kinds = [...new Set(mask.parts.map((p) => t(p.label)))].join(", ");
    const row = el("div", { class: "mask-row" + (sel ? " selected" : ""), title: t("{kinds}. Drag to reorder.", { kinds }), draggable: "true" },
      eye, name, el("span", { class: "hint mask-kinds" }, kinds), del);
    row.addEventListener("click", () => selectMask(mask.index));
    // Drag a row onto another to move it there.
    row.addEventListener("dragstart", (e) => {
      e.dataTransfer.setData("text/x-amb-mask", String(mask.index));
      e.dataTransfer.effectAllowed = "move";
    });
    row.addEventListener("dragover", (e) => {
      if (!e.dataTransfer.types.includes("text/x-amb-mask")) return;
      e.preventDefault();
      row.classList.add("drop");
    });
    row.addEventListener("dragleave", () => row.classList.remove("drop"));
    row.addEventListener("drop", (e) => {
      e.preventDefault();
      row.classList.remove("drop");
      const from = Number(e.dataTransfer.getData("text/x-amb-mask"));
      moveMask(from, mask.index);
    });
    return row;
  });
  $("mask-list").replaceChildren(...rows);
  if (!list.length) $("mask-list").append(el("p", { class: "hint" }, t("No masks yet. Pick a kind above, then draw on the photo.")));
  if (ui.state?.masks?.legacy) {
    $("mask-list").append(el("p", { class: "hint" },
      t("This photo also has older Lightroom masks. They are shown in the photo, but can't be edited here yet.")));
  }

  const detail = $("mask-detail");
  detail.hidden = !m;
  if (m) renderMaskDetail(m);
  renderToolOptions();
  drawMasks();
}

function renderMaskDetail(m) {
  // Duplicate and move.
  const count = maskList().length;
  const action = (text, title, disabled, onclick) => el("button", { type: "button", class: "small", title, disabled, onclick }, text);
  $("mask-actions").replaceChildren(
    action(t("Duplicate"), t("Copy this mask, sliders and all"), false, () => duplicateMask(m.index)),
    action("↑", t("Move up the list"), m.index === 0, () => moveMask(m.index, m.index - 1)),
    action("↓", t("Move down the list"), m.index >= count - 1, () => moveMask(m.index, m.index + 1)),
  );
  // Parts.
  const parts = m.parts.map((p) => {
    const sel = p.index === mk.part;
    const mode = el("select", { title: t("How this part joins the ones above it"), disabled: p.index === 0 && p.mode === "add" });
    for (const [v, text] of MASK_MODES) mode.append(el("option", { value: v }, t(text)));
    mode.value = p.mode;
    mode.addEventListener("click", (e) => e.stopPropagation());
    mode.addEventListener("change", () => run([{ cmd: "set_mask", mask: m.index, part: p.index, values: { mode: mode.value } }]));
    const inv = el("input", { type: "checkbox", title: t("Invert this part"), checked: p.invert });
    inv.addEventListener("click", (e) => e.stopPropagation());
    inv.addEventListener("change", () => run([{ cmd: "set_mask", mask: m.index, part: p.index, values: { invert: inv.checked } }]));
    const del = el("button", { type: "button", class: "small icon-x", title: t("Remove this part") }, "×");
    del.addEventListener("click", (e) => {
      e.stopPropagation();
      run([{ cmd: "remove_mask", mask: m.index, part: p.index }]);
    });
    // Parts combine in order: move one up or down.
    const move = (dir) => {
      const b = el("button", { type: "button", class: "small", title: dir < 0 ? t("Move this part up") : t("Move this part down"),
        disabled: m.parts.length < 2 || (dir < 0 ? p.index === 0 : p.index === m.parts.length - 1) }, dir < 0 ? "↑" : "↓");
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        movePart(m, p.index, p.index + dir);
      });
      return b;
    };
    const label = p.rendered ? t(p.label) : t("{label} (not rendered yet)", { label: t(p.label) });
    const row = el("div", { class: "mask-part" + (sel ? " selected" : "") },
      el("span", { class: "mask-name" }, label), mode, el("label", { class: "inline-check", title: t("Invert") }, inv, t("Inv")),
      move(-1), move(1), del);
    row.addEventListener("click", () => selectPart(p.index));
    return row;
  });
  $("mask-parts").replaceChildren(...parts);

  // Add a part.
  const addRow = $("mask-add");
  const modeSel = el("select", { title: t("How the new part joins this mask") });
  for (const [v, text] of MASK_MODES) modeSel.append(el("option", { value: v }, t(text)));
  modeSel.value = mk.addMode;
  modeSel.addEventListener("change", () => (mk.addMode = modeSel.value));
  addRow.replaceChildren(modeSel, ...NEW_KINDS.map((k) => el("button", {
    type: "button",
    "data-ai": aiFreeUses(k.kind),
    class: "small" + (mk.tool === k.kind && !mk.creatingNew ? " active" : ""),
    title: t("{label} as a new part of this mask. {hint}", { label: t(k.label), hint: t(k.hint) }),
    onclick: () => startTool(k.kind, false),
  }, t(k.label))));

  // Part settings.
  const p = selectedPart();
  const box = $("mask-part-settings");
  box.replaceChildren();
  const setPart = (values) => run([{ cmd: "set_mask", mask: m.index, part: p.index, values }]);
  if (p?.kind === "radial") {
    box.append(
      sliderRow({ ...maskRange("Feather"), value: p.Feather ?? maskDefault("Feather"), onInput: (v) => draftPart({ Feather: v }), onChange: (v) => setPart({ Feather: v }) }),
      sliderRow({ ...maskRange("Angle"), value: p.Angle ?? maskDefault("Angle"), title: t("Turns the ellipse"), onInput: (v) => draftPart({ Angle: v }), onChange: (v) => setPart({ Angle: v }) }),
    );
  } else if (p?.kind === "luminance") {
    const r = p.LumRange ?? [0.5, 0.6, 1, 1];
    const from = Math.round(r[1] * 100);
    const to = Math.round(r[2] * 100);
    const smooth = Math.round(Math.max(r[1] - r[0], r[3] - r[2]) * 100);
    const range = (f, t, s) => {
      const lo = Math.min(f, t) / 100;
      const hi = Math.max(f, t) / 100;
      const sm = s / 100;
      return [Math.max(0, lo - sm), lo, hi, Math.min(1, hi + sm)];
    };
    let cur = { f: from, t: to, s: smooth };
    const draft = () => draftPart({ LumRange: range(cur.f, cur.t, cur.s) });
    const commit = () => setPart({ LumRange: range(cur.f, cur.t, cur.s) });
    box.append(
      // From, To and Smoothness are typed here: a percent view of LumRange, not a protocol field range.
      sliderRow({ label: t("From"), min: 0, max: 100, step: 1, value: from, title: t("Darkest brightness selected"), onInput: (v) => { cur.f = v; draft(); }, onChange: (v) => { cur.f = v; commit(); } }),
      sliderRow({ label: t("To"), min: 0, max: 100, step: 1, value: to, title: t("Brightest brightness selected"), onInput: (v) => { cur.t = v; draft(); }, onChange: (v) => { cur.t = v; commit(); } }),
      sliderRow({ label: t("Smoothness"), min: 0, max: 50, step: 1, value: smooth, title: t("How gradually the selection fades out"), onInput: (v) => { cur.s = v; draft(); }, onChange: (v) => { cur.s = v; commit(); } }),
    );
  } else if (p?.kind === "color") {
    box.append(
      sliderRow({ ...maskRange("ColorAmount"), value: p.ColorAmount ?? maskDefault("ColorAmount"), title: t("Lower selects only colours close to the samples"), onInput: (v) => draftPart({ ColorAmount: v }), onChange: (v) => setPart({ ColorAmount: v }) }),
      el("div", { class: "crop-line" },
        el("button", { type: "button", class: "small", onclick: () => startTool("color", false, true) }, t("Add a sample")),
        el("button", { type: "button", class: "small", onclick: () => setPart({ ColorSamples: [] }) }, t("Clear samples")),
        el("span", { class: "hint" }, t("{n} sample(s)", { n: (p.ColorSamples ?? []).length }))),
    );
  } else if (p?.kind === "brush") {
    box.append(el("div", { class: "crop-line" },
      el("button", { type: "button", class: "small" + (mk.tool === "brush" && mk.continuePart ? " active" : ""), onclick: () => startTool("brush", false, true) }, t("Paint more")),
      el("span", { class: "hint" }, t("{n} stroke(s)", { n: p.strokes }))));
  }

  // The mask's sliders, and the Effect menu that sets them all at once.
  const sl = $("mask-sliders");
  sl.replaceChildren(effectRow(m));
  sl.append(sliderRow({
    ...maskRange("amount"), value: m.amount, title: t("Scales all of this mask's sliders"),
    onInput: (v) => draftCorrection({ CorrectionAmount: fmt6(v / 100) }),
    onChange: (v) => run([{ cmd: "set_mask", mask: m.index, values: { amount: v } }]),
  }));
  let section = null;
  for (const s of mk.sliders) {
    if (s.section !== section) {
      section = s.section;
      sl.append(el("div", { class: "section-label" }, t(section)));
    }
    const value = m.sliders[s.tag] ?? (s.tag === "LocalToningHue" ? 0 : 0);
    const scale = s.tag === "LocalExposure2012" ? 4 : s.tag === "LocalToningHue" ? 1 : 100;
    sl.append(sliderRow({
      label: t(s.label), min: s.min, max: s.max, step: s.step, value,
      title: s.rendered ? s.tag : t("{tag}: kept in the file, not rendered yet", { tag: s.tag }),
      disabled: !s.rendered && !value,
      onInput: (v) => draftCorrection({ [s.tag]: fmt6(v / scale) }),
      onChange: (v) => run([{ cmd: "set_mask", mask: m.index, values: { [s.tag]: v } }]),
    }));
  }
}

// Effect: named slider settings, ours and the user's. Picking one sets every
// slider (the ones it doesn't name go to 0), as Lightroom's Effect menu does.
function effectRow(m) {
  const fx = mk.effects ?? { builtin: [], mine: [] };
  const select = el("select", { title: t("Set this mask's sliders from a saved effect") });
  select.append(el("option", { value: "" }, t("Effect…")));
  const group = (label, list, prefix) => {
    if (!list.length) return;
    const g = el("optgroup", { label });
    for (const e of list) g.append(el("option", { value: prefix + e.name, translate: prefix === "u:" ? "no" : null }, prefix === "b:" ? t(e.name) : e.name));
    select.append(g);
  };
  group("Ambrotype", fx.builtin, "b:");
  group(t("Yours"), fx.mine, "u:");
  // The last one picked stays shown, so it can be deleted.
  if ([...select.options].some((o) => o.value === mk.effect)) select.value = mk.effect;
  select.addEventListener("change", () => {
    const v = select.value;
    mk.effect = v;
    const e = (v.startsWith("b:") ? fx.builtin : fx.mine).find((x) => x.name === v.slice(2));
    if (e) applyEffect(m, e);
  });
  const save = el("button", { type: "button", class: "small", title: t("Keep this mask's sliders as an effect of your own") }, t("Save…"));
  save.addEventListener("click", async () => {
    const name = await askName(t("Save as an effect"), t("This mask's sliders, to use on other masks."));
    if (!name) return;
    const values = Object.fromEntries(Object.entries(m.sliders).filter(([, v]) => Number(v) !== 0));
    try {
      mk.effects = await invoke("save_mask_effect", { name, values });
      mk.effect = `u:${name}`;
      toast(t('Saved effect "{name}"', { name }));
      renderMaskPanel();
    } catch (err) {
      showError(err);
    }
  });
  const mine = select.value.startsWith("u:");
  const del = el("button", { type: "button", class: "small", title: mine ? t("Delete this effect of yours") : t("Pick one of your own effects to delete it"), disabled: !mine }, t("Delete"));
  del.addEventListener("click", async () => {
    mk.effects = await invoke("delete_mask_effect", { name: select.value.slice(2) }).catch((err) => (showError(err), mk.effects));
    mk.effect = "";
    renderMaskPanel();
  });
  return el("div", { class: "crop-line mask-effect" }, select, save, del);
}

function applyEffect(m, effect) {
  const values = {};
  for (const s of mk.sliders) {
    if (!s.rendered && !(s.tag in effect.values)) continue;
    values[s.tag] = effect.values[s.tag] ?? 0;
  }
  run([{ cmd: "set_mask", mask: m.index, values }], `Effect: ${effect.name}`);
}

function renderToolOptions() {
  const box = $("mask-tool");
  box.replaceChildren();
  const k = NEW_KINDS.find((n) => n.kind === mk.tool);
  $("mask-hint").textContent = k
    ? t("{label}: {hint} Esc stops.", { label: t(k.label), hint: t(k.hint) })
    : maskList().length
      ? t("Click a mask to edit it; drag its handles on the photo. O shows or hides the overlay.")
      : "";
  if (mk.tool !== "brush") return;
  const b = mk.brush;
  box.append(
    // Size stays typed: narrower than the 0.1 to 50 the protocol accepts, on purpose.
    sliderRow({ label: t("Size"), min: 0.5, max: 25, step: 0.5, value: b.size, title: t("Radius, % of the photo's width ([ and ])"), onInput: (v) => { b.size = v; drawMasks(); }, onChange: (v) => { b.size = v; drawMasks(); } }),
    sliderRow({ ...maskRange("Feather"), value: b.feather, onInput: (v) => { b.feather = v; drawMasks(); }, onChange: (v) => (b.feather = v) }),
    sliderRow({ ...maskRange("Flow"), value: b.flow, title: t("How fast paint builds up"), onChange: (v) => (b.flow = v) }),
    sliderRow({ ...maskRange("Density"), value: b.density, title: t("Most strength the brush reaches"), onChange: (v) => (b.density = v) }),
  );
  const erase = el("input", { type: "checkbox", checked: b.erase });
  erase.addEventListener("change", () => (b.erase = erase.checked));
  box.append(el("label", { class: "inline-check", title: t("Or hold Alt while painting") }, erase, t("Erase")));
}

// ---------------------------------------------------------------- drafts

function draftCorrection(fields) {
  if (mk.selected == null) return;
  const cs = mk.draft ?? correctionsCopy();
  Object.assign(cs[mk.selected], fields);
  previewDraft(cs);
}

// Edit the selected part's stored fields in a draft (UI values → stored).
function draftPart(values) {
  const p = selectedPart();
  if (!p) return;
  const cs = mk.draft ?? correctionsCopy();
  const [e] = partEntries(cs, mk.selected, p.index);
  if (!e) return;
  for (const [k, v] of Object.entries(values)) {
    if (k === "LumRange") (e.CorrectionRangeMask ??= {}).LumRange = v.map((x) => x.toFixed(6)).join(" ");
    else if (k === "ColorAmount") (e.CorrectionRangeMask ??= {}).ColorAmount = fmt6(v / 100);
    else e[k] = fmt6(v);
  }
  previewDraft(cs);
}

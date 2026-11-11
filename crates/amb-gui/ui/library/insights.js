// SPDX-License-Identifier: GPL-3.0-or-later
// Insights: how the photos shown were shot, a library view beside Timeline
// and Map. A chart each for aperture, focal length, shutter speed, ISO,
// time of day, camera and lens, with how many of each were kept (picked,
// or three stars and up) as the bar's darker part.
//
// - It counts whatever the library shows: a folder, several, a collection,
//   All photos, the filters. Camera chips narrow it to one camera.
// - Clicking a bar shows just those photos in the grid ("f/1.4 ×" in the
//   bar takes it off again).
// - The buckets come from amb_library::insights, so `amb insights` agrees.
"use strict";

const insights = { fields: null, camera: null };

// A photo's number for a field (as amb_library::insights::value).
function insightValue(key, p) {
  if (key === "hour") {
    const h = Number(p.captured?.slice(11, 13));
    return p.captured?.length >= 13 && Number.isFinite(h) ? h : null;
  }
  const v = p[key];
  return Number.isFinite(v) && v > 0 ? v : null;
}

const insightKept = (p) => p.flag === "pick" || (p.rating ?? 0) >= 3;

function insightBucket(field, v) {
  const i = field.buckets.findIndex((b) => b.max == null || v <= b.max);
  return i < 0 ? field.buckets.length - 1 : i;
}

// Show only the photos in one bar, in the grid; null shows them all again.
// `toGrid` false keeps the view (Back uses it).
function setInsightFilter(pick, toGrid = true) {
  lib.pick = pick;
  const chip = $("lib-pick");
  chip.hidden = !pick;
  chip.textContent = pick ? `${pick.words} ×` : "";
  // One step for Back, not one for the filter and one for the view.
  const held = nav.hold;
  nav.hold = true;
  try {
    renderLibrary();
    if (pick && toGrid) setLibraryView("grid");
  } finally {
    nav.hold = held;
  }
  navNote();
}

// The filter for one bar: a number field's bucket, or a camera or lens.
function insightPick(key, label, test) {
  const camera = insights.camera;
  return {
    words: camera && key !== "camera" ? `${label} · ${camera}` : label,
    test: (p) => (!camera || p.camera === camera) && test(p),
  };
}

// Count `photos` by one key, as amb_library::insights::tally does.
function insightTally(key, photos) {
  if (key === "camera" || key === "lens") {
    const by = new Map();
    let unknown = 0;
    for (const p of photos) {
      const name = p[key];
      if (!name) { unknown++; continue; }
      const b = by.get(name) ?? { label: name, count: 0, kept: 0, test: (q) => q[key] === name };
      b.count++;
      b.kept += insightKept(p) ? 1 : 0;
      by.set(name, b);
    }
    const bars = [...by.values()].sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
    return { key, name: key === "camera" ? "Camera" : "Lens", bars, unknown };
  }
  const field = insights.fields.find((f) => f.key === key);
  let bars = field.buckets.map((b, i) => ({ label: b.label, count: 0, kept: 0, test: (q) => { const v = insightValue(key, q); return v != null && insightBucket(field, v) === i; } }));
  let unknown = 0;
  for (const p of photos) {
    const v = insightValue(key, p);
    if (v == null) { unknown++; continue; }
    const b = bars[insightBucket(field, v)];
    b.count++;
    b.kept += insightKept(p) ? 1 : 0;
  }
  if (key !== "hour") {
    const first = bars.findIndex((b) => b.count);
    const last = bars.findLastIndex((b) => b.count);
    bars = first < 0 ? [] : bars.slice(first, last + 1);
  }
  return { key, name: field.name, bars, unknown };
}

async function renderInsights(box) {
  if (!insights.fields) {
    box.replaceChildren(Object.assign(document.createElement("p"), { className: "hint browse-empty", textContent: "Counting…" }));
    try {
      insights.fields = await invoke("insights_fields");
    } catch (e) {
      box.replaceChildren(Object.assign(document.createElement("p"), { className: "hint browse-empty", textContent: String(e) }));
      return;
    }
    if (cull.view !== "insights") return;
  }
  const all = lib.beforePick ?? lib.shown;
  const cameras = insightTally("camera", all).bars;
  if (insights.camera && !cameras.some((c) => c.label === insights.camera)) insights.camera = null;
  const photos = insights.camera ? all.filter((p) => p.camera === insights.camera) : all;
  if (!photos.length) {
    box.replaceChildren(Object.assign(document.createElement("p"), { className: "hint browse-empty", textContent: "No photos shown to count." }));
    return;
  }
  // Head: how many, the most used settings, and the camera chips.
  const head = Object.assign(document.createElement("div"), { className: "in-head" });
  const kept = photos.filter(insightKept).length;
  head.append(Object.assign(document.createElement("b"), { textContent: `${photos.length} photos` }),
    Object.assign(document.createElement("span"), { className: "hint", textContent: `${kept} kept (${Math.round((100 * kept) / photos.length)} %): picked, or 3 stars and up` }));
  const usual = insightUsual(photos);
  if (usual) head.append(Object.assign(document.createElement("span"), { className: "in-usual", textContent: `Most often: ${usual}`, title: "The settings you shoot with most, together" }));
  if (cameras.length > 1) {
    const chips = Object.assign(document.createElement("div"), { className: "in-chips", role: "group", ariaLabel: "Camera" });
    for (const c of [{ label: null, count: all.length }, ...cameras]) {
      const b = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: `${c.label ?? "All cameras"} ${c.count}` });
      b.classList.toggle("on", c.label === insights.camera);
      b.addEventListener("click", () => {
        insights.camera = c.label;
        renderInsights(box);
      });
      chips.append(b);
    }
    head.append(chips);
  }
  const charts = Object.assign(document.createElement("div"), { className: "in-charts" });
  for (const f of insights.fields) charts.append(insightChart(insightTally(f.key, photos), photos.length));
  for (const key of ["lens", "camera"]) {
    if (key === "camera" && (insights.camera || cameras.length < 2)) continue;
    charts.append(insightList(insightTally(key, photos), photos.length));
  }
  box.replaceChildren(head, charts);
}

// The most common aperture, focal length, shutter and ISO, together.
function insightUsual(photos) {
  const keys = ["focal", "aperture", "shutter", "iso"];
  const counts = new Map();
  for (const p of photos) {
    const parts = keys.map((k) => {
      const v = insightValue(k, p);
      const f = insights.fields.find((x) => x.key === k);
      return v == null ? null : f.buckets[insightBucket(f, v)].label;
    });
    if (parts.some((x) => x == null)) continue;
    const words = `${parts.slice(0, 3).join(" · ")} · ISO ${parts[3]}`;
    counts.set(words, (counts.get(words) ?? 0) + 1);
  }
  let best = null;
  for (const [w, n] of counts) if (!best || n > best[1]) best = [w, n];
  return best && best[1] > 1 ? `${best[0]} (${best[1]}×)` : null;
}

function insightCard(t) {
  const card = Object.assign(document.createElement("section"), { className: `in-card in-${t.key}` });
  card.append(Object.assign(document.createElement("h4"), { textContent: t.name }));
  return card;
}

function insightFoot(card, t) {
  if (t.unknown) card.append(Object.assign(document.createElement("p"), { className: "hint in-unknown", textContent: `${t.unknown} without` }));
}

// A number field as upright bars.
function insightChart(t, total) {
  const card = insightCard(t);
  const most = Math.max(1, ...t.bars.map((b) => b.count));
  const bars = Object.assign(document.createElement("div"), { className: "in-bars" });
  for (const b of t.bars) {
    const pct = Math.round((100 * b.count) / total);
    const col = Object.assign(document.createElement("button"), { type: "button", className: "in-bar", disabled: !b.count });
    col.title = b.count ? `${b.label}: ${b.count} photos (${pct} %), ${b.kept} kept. Click to show them.` : `${b.label}: none`;
    const fill = Object.assign(document.createElement("span"), { className: "in-fill" });
    fill.style.height = `${(b.count / most) * 100}%`;
    const keep = Object.assign(document.createElement("span"), { className: "in-kept" });
    keep.style.height = b.count ? `${(b.kept / b.count) * 100}%` : "0";
    fill.append(keep);
    const label = t.key === "iso" ? `ISO ${b.label}` : t.key === "hour" ? `${b.label}:00` : b.label;
    col.append(fill, Object.assign(document.createElement("span"), { className: "in-label", textContent: b.label }));
    if (b.count) col.addEventListener("click", () => setInsightFilter(insightPick(t.key, label, b.test)));
    bars.append(col);
  }
  card.append(bars);
  insightFoot(card, t);
  return card;
}

// Cameras and lenses as a list of sideways bars, the most used first.
function insightList(t, total) {
  const card = insightCard(t);
  const most = Math.max(1, ...t.bars.map((b) => b.count));
  const list = Object.assign(document.createElement("div"), { className: "in-list" });
  for (const b of t.bars.slice(0, 12)) {
    const row = Object.assign(document.createElement("button"), { type: "button", className: "in-row" });
    row.title = `${b.count} photos (${Math.round((100 * b.count) / total)} %), ${b.kept} kept. Click to show them.`;
    const track = Object.assign(document.createElement("span"), { className: "in-track" });
    const fill = Object.assign(document.createElement("span"), { className: "in-fill" });
    fill.style.width = `${(b.count / most) * 100}%`;
    const keep = Object.assign(document.createElement("span"), { className: "in-kept" });
    keep.style.width = `${(b.kept / b.count) * 100}%`;
    fill.append(keep);
    track.append(fill);
    row.append(Object.assign(document.createElement("span"), { className: "in-name", textContent: b.label }), track,
      Object.assign(document.createElement("span"), { className: "in-n", textContent: b.count }));
    row.addEventListener("click", () => setInsightFilter(insightPick(t.key, b.label, b.test)));
    list.append(row);
  }
  if (t.bars.length > 12) list.append(Object.assign(document.createElement("p"), { className: "hint", textContent: `and ${t.bars.length - 12} more` }));
  card.append(list);
  insightFoot(card, t);
  return card;
}

function wireInsights() {
  $("lib-pick").addEventListener("click", () => setInsightFilter(null));
}

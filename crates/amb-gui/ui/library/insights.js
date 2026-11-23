// SPDX-License-Identifier: GPL-3.0-or-later
// Insights: how the photos shown were shot, a library view beside Timeline
// and Map. A chart each for aperture, focal length, shutter speed, ISO,
// time of day, camera and lens, with how many of each were kept (picked,
// or three stars and up) as the bar's darker part.
//
// - It counts whatever the library shows: a folder, several, a collection,
//   All photos, the filters. Camera chips narrow it to one camera.
// - Clicking a bar shows just those photos in the grid ("f/1.4 · Show all ×"
//   in the bar, Back or Esc takes it off again).
// - Split by camera, lens or year puts up to three side by side, each as a
//   share of its own photos, so a small group compares fairly.
// - Above the charts, a summary of what the numbers say you shoot, what
//   they suggest (a fast lens kept stopped down, a zoom at its end, shake,
//   dark scenes, where the keepers come from), gear tied to a shortfall and
//   challenges to try; all from amb_library::insights.
// - The buckets come from amb_library::insights, so `amb insights` agrees.
"use strict";

const insights = { fields: null, camera: null, split: null, notes: null, notesOf: null };

// Split by: a group's name for a photo.
const INSIGHT_SPLITS = { camera: (p) => p.camera, lens: (p) => p.lens, year: (p) => p.captured?.slice(0, 4) };
const INSIGHT_COLOURS = ["var(--accent)", "#e0a040", "#50c090"];

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
  chip.textContent = pick ? filterChipWords(pick.words) : "";
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
    return { key, name: key === "camera" ? tKey("Camera") : tKey("Lens"), bars, unknown };
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
    box.replaceChildren(el("p", { class: "hint browse-empty" }, t("Counting…")));
    try {
      insights.fields = await invoke("insights_fields");
    } catch (e) {
      box.replaceChildren(el("p", { class: "hint browse-empty" }, String(e)));
      return;
    }
    if (cull.view !== "insights") return;
  }
  const all = lib.beforePick ?? lib.shown;
  const cameras = insightTally("camera", all).bars;
  if (insights.camera && !cameras.some((c) => c.label === insights.camera)) insights.camera = null;
  const photos = insights.camera ? all.filter((p) => p.camera === insights.camera) : all;
  if (!photos.length) {
    box.replaceChildren(el("p", { class: "hint browse-empty" }, t("No photos shown to count.")));
    return;
  }
  // Head: how many, the most used settings, and the camera chips.
  const head = el("div", { class: "in-head" });
  const kept = photos.filter(insightKept).length;
  head.append(el("b", {}, plural(photos.length, "photo")),
    el("span", { class: "hint" }, t("{kept} kept ({pct} %): picked, or 3 stars and up", { kept, pct: Math.round((100 * kept) / photos.length) })));
  const usual = insightUsual(photos);
  if (usual) head.append(el("span", { class: "in-usual", title: t("The settings you shoot with most, together") }, t("Most often: {usual}", { usual })));
  if (cameras.length > 1) {
    const chips = el("div", { class: "in-chips", role: "group", "aria-label": t("Camera") });
    for (const c of [{ label: null, count: all.length }, ...cameras]) {
      const b = el("button", { type: "button", class: "small", translate: c.label ? "no" : null }, c.label ? `${c.label} ${c.count}` : t("All cameras {n}", { n: c.count }));
      b.classList.toggle("on", c.label === insights.camera);
      b.addEventListener("click", () => {
        insights.camera = c.label;
        renderInsights(box);
      });
      chips.append(b);
    }
    head.append(chips);
  }
  // Split by: the three biggest groups side by side.
  const split = el("label", { class: "in-split" }, t("Split by "));
  const pick = document.createElement("select");
  for (const [v, w] of [["", tKey("nothing")], ["camera", tKey("camera")], ["lens", tKey("lens")], ["year", tKey("year")]]) pick.append(new Option(t(w), v, false, v === (insights.split ?? "")));
  pick.addEventListener("change", () => {
    insights.split = pick.value || null;
    renderInsights(box);
  });
  split.append(pick);
  head.append(split);
  const series = insightSeries(photos);
  if (series) {
    const legend = el("div", { class: "in-legend" });
    for (const s of series) {
      const k = el("span", { class: "in-key", translate: "no" });
      k.style.setProperty("--c", s.colour);
      k.textContent = `${s.name} (${s.photos.length})`;
      legend.append(k);
    }
    head.append(legend);
  }
  const notes = el("div", { class: "in-notes" });
  insightNotes(photos, notes);
  const charts = el("div", { class: "in-charts" });
  for (const f of insights.fields) {
    charts.append(series ? insightSplitChart(f.key, series) : insightChart(insightTally(f.key, photos), photos.length));
  }
  for (const key of ["lens", "camera"]) {
    if (key === "camera" && (insights.camera || cameras.length < 2)) continue;
    charts.append(insightList(insightTally(key, photos), photos.length));
  }
  box.replaceChildren(head, notes, charts);
}

// The groups to split by: the three with the most photos, or null.
function insightSeries(photos) {
  const of = INSIGHT_SPLITS[insights.split];
  if (!of) return null;
  const groups = new Map();
  for (const p of photos) {
    const g = of(p);
    if (!g) continue;
    if (!groups.has(g)) groups.set(g, []);
    groups.get(g).push(p);
  }
  const top = [...groups].sort((a, b) => b[1].length - a[1].length).slice(0, 3);
  if (top.length < 2) return null;
  return top.map(([name, ps], i) => ({ name, photos: ps, colour: INSIGHT_COLOURS[i], test: (q) => of(q) === name }));
}

// A number field with a bar per group in each bucket, as shares of the group.
function insightSplitChart(key, series) {
  const tallies = series.map((s) => insightTally(key, s.photos));
  const card = insightCard(tallies[0]);
  const labels = insights.fields.find((f) => f.key === key).buckets.map((b) => b.label);
  const used = labels.filter((l) => tallies.some((tally) => tally.bars.find((b) => b.label === l)?.count));
  const shown = key === "hour" ? labels : labels.slice(labels.indexOf(used[0]), labels.indexOf(used.at(-1)) + 1);
  const share = (tally, s, l) => (tally.bars.find((b) => b.label === l)?.count ?? 0) / Math.max(1, s.photos.length);
  const most = Math.max(0.0001, ...shown.flatMap((l) => tallies.map((tally, i) => share(tally, series[i], l))));
  const bars = el("div", { class: "in-bars" });
  for (const l of shown) {
    const col = el("div", { class: "in-bar in-group" });
    const row = el("span", { class: "in-sub" });
    tallies.forEach((tally, i) => {
      const s = series[i];
      const bar = tally.bars.find((b) => b.label === l);
      const b = el("button", { type: "button", class: "in-fill", disabled: !bar?.count });
      b.style.height = `${(share(tally, s, l) / most) * 100}%`;
      b.style.background = s.colour;
      b.title = t("{name}, {label}: {n} of {total} ({pct} %)", { name: s.name, label: l, n: bar?.count ?? 0, total: s.photos.length, pct: Math.round(share(tally, s, l) * 100) });
      if (bar?.count) b.addEventListener("click", () => setInsightFilter({ words: `${l} · ${s.name}`, test: (q) => s.test(q) && bar.test(q) }));
      row.append(b);
    });
    col.append(row, el("span", { class: "in-label" }, l));
    bars.append(col);
  }
  card.append(bars);
  return card;
}

// What the numbers say, suggest and challenge; asked of the app once per set
// of photos (it reads them the way `amb insights` does).
async function insightNotes(photos, box) {
  const key = `${photos.length}|${photos[0]?.path}|${photos.at(-1)?.path}|${photos.filter(insightKept).length}`;
  if (insights.notesOf !== key) {
    insights.notesOf = key;
    insights.notes = null;
    const send = photos.map((p) => ({ captured: p.captured, camera: p.camera, lens: p.lens, iso: p.iso, aperture: p.aperture, shutter: p.shutter, focal: p.focal, rating: p.rating ?? null, flag: p.flag ?? null }));
    try {
      insights.notes = await invoke("insights_advice", { photos: send });
    } catch (e) {
      insights.notes = { summary: null, advice: [], gear: [], challenges: [], error: String(e) };
    }
    if (insights.notesOf !== key) return;
  }
  const n = insights.notes;
  if (!n) return;
  const part = (title, list, cls) => {
    if (!list.length) return null;
    const sec = el("section", { class: `in-card in-advice ${cls}` });
    sec.append(el("h4", {}, t(title)));
    for (const x of list) {
      const p = document.createElement("p");
      p.append(el("b", {}, x.title), document.createElement("br"), x.text);
      sec.append(p);
    }
    return sec;
  };
  // The summary leads: a heading and its one paragraph.
  const lead = n.summary && el("section", { class: "in-card in-advice in-summary" }, el("h4", {}, t("What the numbers say you shoot")), el("p", {}, n.summary.text));
  box.replaceChildren(...[lead, part(tKey("What it suggests"), n.advice, "in-suggest"), part(tKey("Gear to think about"), n.gear, "in-gear"), part(tKey("Try this week"), n.challenges, "in-try")].filter(Boolean));
  if (!box.children.length) box.append(el("p", { class: "hint" }, n.error ?? t("Advice and challenges come with 20 or more photos that say how they were shot.")));
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

function insightCard(tally) {
  const card = el("section", { class: `in-card in-${tally.key}` });
  card.append(el("h4", {}, t(tally.name)));
  return card;
}

function insightFoot(card, tally) {
  if (tally.unknown) card.append(el("p", { class: "hint in-unknown" }, t("{n} without", { n: tally.unknown })));
}

// A number field as upright bars.
function insightChart(tally, total) {
  const card = insightCard(tally);
  const most = Math.max(1, ...tally.bars.map((b) => b.count));
  const bars = el("div", { class: "in-bars" });
  for (const b of tally.bars) {
    const pct = Math.round((100 * b.count) / total);
    const col = el("button", { type: "button", class: "in-bar", disabled: !b.count });
    col.title = b.count ? t("{label}: {n} photos ({pct} %), {kept} kept. Click to show them.", { label: b.label, n: b.count, pct, kept: b.kept }) : t("{label}: none", { label: b.label });
    const fill = el("span", { class: "in-fill" });
    fill.style.height = `${(b.count / most) * 100}%`;
    const keep = el("span", { class: "in-kept" });
    keep.style.height = b.count ? `${(b.kept / b.count) * 100}%` : "0";
    fill.append(keep);
    const label = tally.key === "iso" ? `ISO ${b.label}` : tally.key === "hour" ? `${b.label}:00` : b.label;
    col.append(fill, el("span", { class: "in-label" }, b.label));
    if (b.count) col.addEventListener("click", () => setInsightFilter(insightPick(tally.key, label, b.test)));
    bars.append(col);
  }
  card.append(bars);
  insightFoot(card, tally);
  return card;
}

// Cameras and lenses as a list of sideways bars, the most used first.
function insightList(tally, total) {
  const card = insightCard(tally);
  const most = Math.max(1, ...tally.bars.map((b) => b.count));
  const list = el("div", { class: "in-list" });
  for (const b of tally.bars.slice(0, 12)) {
    const row = el("button", { type: "button", class: "in-row" });
    row.title = t("{n} photos ({pct} %), {kept} kept. Click to show them.", { n: b.count, pct: Math.round((100 * b.count) / total), kept: b.kept });
    const track = el("span", { class: "in-track" });
    const fill = el("span", { class: "in-fill" });
    fill.style.width = `${(b.count / most) * 100}%`;
    const keep = el("span", { class: "in-kept" });
    keep.style.width = `${(b.kept / b.count) * 100}%`;
    fill.append(keep);
    track.append(fill);
    row.append(el("span", { class: "in-name", translate: "no" }, b.label), track,
      el("span", { class: "in-n" }, b.count));
    row.addEventListener("click", () => setInsightFilter(insightPick(tally.key, b.label, b.test)));
    list.append(row);
  }
  if (tally.bars.length > 12) list.append(el("p", { class: "hint" }, t("and {n} more", { n: tally.bars.length - 12 })));
  card.append(list);
  insightFoot(card, tally);
  return card;
}

// Esc in the grid takes an Insights or date filter off (true if it did), so
// the way back to the whole folder is not only the chip or Back.
function libFilterEscape() {
  if (!lib.on || cull.view !== "grid") return false;
  if (lib.pick) setInsightFilter(null);
  else if (lib.date) setDateFilter(null);
  else return false;
  return true;
}

function wireInsights() {
  $("lib-pick").addEventListener("click", () => setInsightFilter(null));
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Rating, colour label, flag and keywords for the open photo. They're
// written to the photo's XMP sidecar as soon as they change (xmp:Rating,
// xmp:Label and dc:subject, where Lightroom keeps them, and our amblib:Flag;
// src/session.rs `set_info`), and embedded in exports. Keys 1–5 rate, 0
// clears, 6–9 label, P/X/U flag (keys.js); the keyword field takes Enter or
// commas.
"use strict";

const infoState = { info: { rating: null, keywords: [], label: null, flag: null }, recent: [], busy: Promise.resolve() };

const FLAG_DONE = { pick: "Flagged as a pick", reject: "Flagged as rejected", "": "Flag removed" };

// A flag mark: a white flag for a pick, a crossed one for a reject.
function showFlag(el, flag) {
  el.textContent = flag === "pick" ? "⚑" : flag === "reject" ? "✕" : "";
  el.className = `${el.className.split(" ")[0]}${flag ? ` ${flag}` : ""}`;
  el.title = flag === "pick" ? "Pick (P)" : flag === "reject" ? "Rejected (X)" : "";
}

function renderInfo() {
  const block = $("block-info");
  const on = Boolean(ui.info);
  block.classList.toggle("disabled", !on);
  const rating = infoState.info.rating ?? 0;
  for (const b of $("rating").querySelectorAll("button[data-stars]")) {
    const n = Number(b.dataset.stars);
    b.classList.toggle("on", n <= rating);
    b.setAttribute("aria-checked", String(n === rating));
    b.disabled = !on;
  }
  $("rating-clear").disabled = !on || !rating;
  for (const b of $("label-swatches").querySelectorAll("button")) {
    b.classList.toggle("on", b.dataset.label === infoState.info.label);
    b.disabled = !on;
  }
  for (const b of $("flag-buttons").querySelectorAll("button")) {
    b.classList.toggle("on", b.dataset.flag === (infoState.info.flag ?? ""));
    b.disabled = !on;
  }
  const chips = $("tag-chips");
  chips.replaceChildren(...infoState.info.keywords.map((k) => {
    const chip = Object.assign(document.createElement("span"), { className: "tag-chip", textContent: k });
    const x = Object.assign(document.createElement("button"), { type: "button", textContent: "×", title: `Remove "${k}"` });
    x.addEventListener("click", () => setKeywords(infoState.info.keywords.filter((y) => y !== k)));
    chip.append(x);
    return chip;
  }));
  $("tag-input").disabled = !on;
  for (const f of $("info-meta").querySelectorAll("[data-meta]")) {
    f.disabled = !on;
    if (document.activeElement !== f) f.value = infoState.info[f.dataset.meta] ?? "";
  }
  const have = new Set(infoState.info.keywords.map((k) => k.toLowerCase()));
  $("tag-suggest").replaceChildren(...infoState.recent.filter((k) => !have.has(k.toLowerCase())).slice(0, 50)
    .map((k) => Object.assign(document.createElement("option"), { value: k })));
}

async function loadInfo() {
  try {
    const r = await invoke("photo_info");
    infoState.info = r.info;
    infoState.recent = r.recent;
  } catch (e) {
    toast(String(e), true);
  }
  renderInfo();
}

// Changes are written one after another, so a fast 3-then-4 ends as 4.
function saveInfo(info) {
  infoState.info = info;
  renderInfo();
  infoState.busy = infoState.busy.then(async () => {
    try {
      const r = await invoke("set_photo_info", { info });
      infoState.recent = r.recent;
      if (infoState.info === info) infoState.info = r.info;
    } catch (e) {
      toast(`Couldn't save the rating and keywords: ${e}`, true);
    }
    renderInfo();
  });
  return infoState.busy;
}

function setRating(n) {
  if (!ui.info) return;
  const rating = n >= 1 && n <= 5 ? n : null;
  saveInfo({ ...infoState.info, rating });
  stripUpdateCurrent({ rating });
  status(rating ? `Rated ${"★".repeat(rating)}` : "Rating cleared");
}

// The same label again removes it.
function setLabel(label) {
  if (!ui.info) return;
  const next = infoState.info.label === label ? null : label;
  saveInfo({ ...infoState.info, label: next });
  stripUpdateCurrent({ label: next });
  status(next ? `${next} label` : "Label removed");
}

// "pick", "reject", or "" for none.
function setFlag(flag) {
  if (!ui.info) return;
  saveInfo({ ...infoState.info, flag: flag || null });
  stripUpdateCurrent({ flag: flag || null });
  status(FLAG_DONE[flag]);
}

function setKeywords(keywords) {
  if (!ui.info) return;
  const seen = new Set();
  const clean = keywords.map((k) => k.trim()).filter((k) => k && !seen.has(k.toLowerCase()) && seen.add(k.toLowerCase()));
  saveInfo({ ...infoState.info, keywords: clean });
}

// Whatever is typed becomes keywords (commas separate them).
function addTyped() {
  const input = $("tag-input");
  const parts = input.value.split(",").map((k) => k.trim()).filter(Boolean);
  input.value = "";
  if (parts.length) setKeywords([...infoState.info.keywords, ...parts]);
}

function wireInfo() {
  for (const b of $("rating").querySelectorAll("button[data-stars]")) {
    const n = Number(b.dataset.stars);
    // Clicking the current rating again clears it, like Lightroom.
    b.addEventListener("click", () => setRating((infoState.info.rating ?? 0) === n ? 0 : n));
  }
  $("rating-clear").addEventListener("click", () => setRating(0));
  for (const l of LABELS) {
    const b = Object.assign(document.createElement("button"), { type: "button", title: `${l} label (${LABELS.indexOf(l) < 4 ? 6 + LABELS.indexOf(l) : "no key"})` });
    b.dataset.label = l;
    b.style.background = LABEL_COLOURS[l];
    b.addEventListener("click", () => setLabel(l));
    $("label-swatches").append(b);
  }
  for (const b of $("flag-buttons").querySelectorAll("button")) b.addEventListener("click", () => setFlag(b.dataset.flag));
  // Title, caption, creator and copyright: saved when the field is left.
  for (const f of $("info-meta").querySelectorAll("[data-meta]")) {
    f.addEventListener("change", () => {
      if (!ui.info) return;
      saveInfo({ ...infoState.info, [f.dataset.meta]: f.value.trim() });
      status(`${f.closest("label").firstChild.textContent.trim()} saved`);
    });
    // Typing in them isn't a shortcut.
    f.addEventListener("keydown", (e) => e.stopPropagation());
  }
  const input = $("tag-input");
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === "," || e.key === "Tab") {
      if (!input.value.trim()) return;
      e.preventDefault();
      addTyped();
    } else if (e.key === "Backspace" && !input.value && infoState.info.keywords.length) {
      setKeywords(infoState.info.keywords.slice(0, -1));
    } else if (e.key === "Escape") {
      input.value = "";
      input.blur();
    }
  });
  // Picking a suggestion from the list adds it straight away.
  input.addEventListener("input", (e) => {
    if (e.inputType === "insertReplacementText" || (!e.inputType && infoState.recent.includes(input.value))) addTyped();
  });
  input.addEventListener("blur", addTyped);
  $("tag-field").addEventListener("click", (e) => { if (e.target === $("tag-field")) input.focus(); });
  renderInfo();
}

// Rating and keywords for the open photo. They're written to the photo's XMP
// sidecar as soon as they change (xmp:Rating and dc:subject, where Lightroom
// keeps them; src/session.rs `set_info`), and embedded in exports.
// Keys 1–5 rate, 0 clears (keys.js); the keyword field takes Enter or commas.
"use strict";

const infoState = { info: { rating: null, keywords: [] }, recent: [], busy: Promise.resolve() };

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
  const chips = $("tag-chips");
  chips.replaceChildren(...infoState.info.keywords.map((k) => {
    const chip = Object.assign(document.createElement("span"), { className: "tag-chip", textContent: k });
    const x = Object.assign(document.createElement("button"), { type: "button", textContent: "×", title: `Remove "${k}"` });
    x.addEventListener("click", () => setKeywords(infoState.info.keywords.filter((y) => y !== k)));
    chip.append(x);
    return chip;
  }));
  $("tag-input").disabled = !on;
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

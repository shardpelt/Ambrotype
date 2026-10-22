// SPDX-License-Identifier: GPL-3.0-or-later
// Photography basics (Help → Photography basics, Shift+F1): a short guide
// to how a photo is made and edited, and the terms the help cards link to.
// In a help card a term such as "midtones" is underlined faintly: hovering
// it says what it means, clicking it opens the guide at that term. Nothing
// shows unless asked for. The text is a data file (basics.json).
"use strict";

const basics = { data: null, loading: null, tip: null };

async function loadBasics() {
  basics.loading ??= fetch("basics.json")
    .then((r) => r.json())
    .then((d) => {
      // Longest first, so "shallow depth of field" wins over "depth of field".
      const forms = [];
      for (const t of d.terms) for (const f of [t.term, ...(t.also ?? [])]) forms.push([f, t]);
      forms.sort((a, b) => b[0].length - a[0].length);
      const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      // Not inside a compound such as "middle-to-dark areas".
      d.pattern = new RegExp(`(?<![-\\w])(${forms.map(([f]) => esc(f)).join("|")})(?![-\\w])`, "gi");
      d.byForm = new Map(forms.map(([f, t]) => [f.toLowerCase(), t]));
      d.byId = new Map(d.terms.map((t) => [t.id, t]));
      basics.data = d;
      return d;
    })
    .catch(() => null);
  return basics.loading;
}

// `text` with its terms as links (each term once per text, and not `skip`,
// the setting the text is about).
function linkTerms(text, skip = "") {
  const out = document.createDocumentFragment();
  const d = basics.data;
  if (!d) {
    out.append(text);
    return out;
  }
  const seen = new Set([skip.toLowerCase()]);
  let at = 0;
  for (const m of text.matchAll(d.pattern)) {
    const t = d.byForm.get(m[0].toLowerCase());
    if (!t || seen.has(t.id) || seen.has(t.term.toLowerCase())) continue;
    seen.add(t.id);
    out.append(text.slice(at, m.index));
    const a = Object.assign(document.createElement("span"), { className: "term", textContent: m[0], tabIndex: 0 });
    a.dataset.term = t.id;
    a.setAttribute("role", "link");
    out.append(a);
    at = m.index + m[0].length;
  }
  out.append(text.slice(at));
  return out;
}

function showTermTip(el) {
  const t = basics.data?.byId.get(el.dataset.term);
  if (!t) return;
  basics.tip ??= Object.assign(document.createElement("div"), { className: "help-tip term-tip", role: "tooltip" });
  basics.tip.replaceChildren(Object.assign(document.createElement("b"), { textContent: t.term }), ` ${t.short}`,
    Object.assign(document.createElement("small"), { textContent: "Click to read more in Photography basics" }));
  document.body.append(basics.tip);
  const r = el.getBoundingClientRect();
  const w = basics.tip.offsetWidth, h = basics.tip.offsetHeight;
  const left = Math.min(Math.max(8, r.left), innerWidth - w - 8);
  const top = r.bottom + 6 + h > innerHeight ? r.top - h - 6 : r.bottom + 6;
  Object.assign(basics.tip.style, { left: `${left}px`, top: `${top}px` });
}

const hideTermTip = () => basics.tip?.remove();

// Terms work wherever they are: help cards and the guide itself.
function wireTerms() {
  loadBasics();
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest?.(".term");
    if (el) showTermTip(el);
  });
  document.addEventListener("mouseout", (e) => {
    if (e.target.closest?.(".term")) hideTermTip();
  });
  const open = (e) => {
    const el = e.target.closest?.(".term");
    if (!el) return;
    e.preventDefault();
    e.stopPropagation();
    hideTermTip();
    closeHelpCard();
    basicsFlow(el.dataset.term);
  };
  document.addEventListener("click", open);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Enter") open(e);
  });
}

// The guide: chapters and terms on the left, the page on the right. `at` is a
// chapter or a term id to open at.
async function basicsFlow(at = null) {
  const d = await loadBasics();
  if (!d) {
    toast("Couldn't load Photography basics", true);
    return;
  }
  const dlg = $("basics-dialog");
  const nav = $("basics-nav");
  const page = $("basics-page");
  const show = (id) => {
    const chapter = d.guide.find((c) => c.id === id);
    const term = !chapter && d.byId.get(id);
    page.replaceChildren();
    if (chapter) {
      page.append(Object.assign(document.createElement("h3"), { textContent: chapter.title }));
      for (const text of chapter.body) {
        const p = document.createElement("p");
        p.append(linkTerms(text));
        page.append(p);
      }
      const i = d.guide.indexOf(chapter);
      const next = d.guide[i + 1];
      if (next) {
        const b = Object.assign(document.createElement("button"), { type: "button", textContent: `Next: ${next.title.replace(/^\d+\.\s*/, "")} →` });
        b.addEventListener("click", () => show(next.id));
        page.append(b);
      }
    } else if (term) {
      page.append(Object.assign(document.createElement("h3"), { textContent: term.term }));
      const short = Object.assign(document.createElement("p"), { className: "basics-short" });
      short.append(linkTerms(term.short, term.term));
      const body = document.createElement("p");
      body.append(linkTerms(term.body, term.term));
      page.append(short, body);
    }
    for (const b of nav.querySelectorAll("button")) b.classList.toggle("active", b.dataset.id === id);
    nav.querySelector(`[data-id="${id}"]`)?.scrollIntoView({ block: "nearest" });
    page.scrollTop = 0;
  };
  nav.replaceChildren();
  const section = (title, items) => {
    nav.append(Object.assign(document.createElement("h4"), { textContent: title }));
    for (const [id, text] of items) {
      const b = Object.assign(document.createElement("button"), { type: "button", textContent: text });
      b.dataset.id = id;
      b.addEventListener("click", () => show(id));
      nav.append(b);
    }
  };
  section("How photography works", d.guide.map((c) => [c.id, c.title]));
  section("Terms", [...d.terms].sort((a, b) => a.term.localeCompare(b.term)).map((t) => [t.id, t.term]));
  if (!dlg.open) dlg.showModal();
  show(at && (d.byId.has(at) || d.guide.some((c) => c.id === at)) ? at : d.guide[0].id);
  page.focus({ preventScroll: true });
}

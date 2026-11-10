// SPDX-License-Identifier: GPL-3.0-or-later
// The blocks in a side column, and the two ways a column scrolls (View menu,
// Settings → General):
// - as one: a block's heading sticks at the top while you're in it, and the
//   blocks further down are named in a strip at the column's foot, a click
//   scrolls there;
// - contained: the column keeps to the window's height, each block scrolls
//   on its own, and the line between two blocks drags to share the height
//   out (remembered on this computer). Presets fill what's left.
// Either way a block's heading folds it (remembered).

const columnWatch = new WeakMap();
const columnsContained = () => store.text("amb.columns") === "contained";

function setColumnsContained(on) {
  store.setText("amb.columns", on ? "contained" : "one");
  const box = $("settings-columns");
  if (box) box.checked = on;
  applyActiveLooks();
  status(on ? "Columns: each block scrolls on its own; drag the lines between them" : "Columns: one scroll, the blocks further down named at the foot");
  return true;
}

function wireColumnBlocks(col) {
  const contained = columnsContained();
  document.body.classList.toggle("cols-contained", contained);
  for (const s of col.querySelectorAll(":scope > .side-section")) foldableBlock(s);
  if (contained) {
    col.style.removeProperty("--col-tabs-h");
    return addBlockDividers(col);
  }
  for (const el of columnBlocks(col)) {
    el.style.height = "";
    el.classList.remove("sized");
  }
  const strip = Object.assign(document.createElement("nav"), { className: "col-strip", ariaLabel: "Further down this column" });
  col.append(strip);
  let w = columnWatch.get(col);
  if (!w) {
    let queued = false;
    const update = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        updateColumnStrip(col);
      });
    };
    w = { update, sizes: new ResizeObserver(update) };
    columnWatch.set(col, w);
    col.addEventListener("scroll", update, { passive: true });
  }
  w.sizes.disconnect();
  w.sizes.observe(col);
  for (const el of columnBlocks(col)) w.sizes.observe(el);
  w.update();
}

// A column's blocks: its sections and panels.
const columnBlocks = (col) => [...col.children].filter((el) => el.matches(".side-section, details.panel"));
const blockName = (el) => el.dataset.label ?? el.querySelector(":scope > .side-head h2, :scope > summary > span")?.textContent.trim() ?? "";

function foldableBlock(s) {
  const h = s.querySelector(":scope > .side-head h2");
  if (!h || !s.id) return;
  const key = `amb.fold.${s.id.replace(/^block-/, "")}`;
  if (!h.querySelector(".fold-head")) {
    const head = Object.assign(document.createElement("button"), { type: "button", className: "fold-head", title: "Fold or unfold" });
    head.append(...h.childNodes);
    h.replaceChildren(head);
    head.addEventListener("click", () => foldBlock(s, !s.classList.contains("folded")));
  }
  foldBlock(s, store.text(key) === "1", false);
}

function foldBlock(s, folded, remember = true) {
  s.classList.toggle("folded", folded);
  s.querySelector(".fold-head")?.setAttribute("aria-expanded", String(!folded));
  if (remember) store.setText(`amb.fold.${s.id.replace(/^block-/, "")}`, folded ? "1" : "0");
}

function updateColumnStrip(col) {
  const strip = col.querySelector(":scope > .col-strip");
  if (!strip) return;
  const tabs = col.querySelector(":scope > .column-tabs");
  col.style.setProperty("--col-tabs-h", `${tabs?.offsetHeight ?? 0}px`);
  const stripH = strip.childElementCount ? strip.offsetHeight : 0;
  col.style.setProperty("--col-strip-h", `${stripH}px`);
  const box = col.getBoundingClientRect();
  // History docked at the foot (style.css) is always in sight, and covers
  // what scrolls behind it.
  const dock = [...col.querySelectorAll(":scope > .side-section.history")].find((el) => getComputedStyle(el).position === "sticky");
  const foot = dock ? dock.getBoundingClientRect().top : box.bottom - stripH;
  const below = columnBlocks(col).filter((el) => el !== dock && el.offsetParent && el.getBoundingClientRect().top >= foot - 8);
  const names = below.map(blockName).filter(Boolean);
  if (strip.dataset.names === names.join("\n")) return;
  strip.dataset.names = names.join("\n");
  // A few by name, then how many more (a click goes to the next of them).
  const named = below.filter((el) => blockName(el));
  const shown = named.length > 5 ? named.slice(0, 4) : named;
  const chips = shown.map((el) => [el, blockName(el)]);
  if (named.length > shown.length) chips.push([named[shown.length], `+${named.length - shown.length}`]);
  strip.replaceChildren(...chips.map(([el, text]) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: "col-chip", textContent: text, title: `Scroll down to ${blockName(el)}` });
    b.addEventListener("click", () => {
      if (el.classList.contains("folded")) foldBlock(el, false);
      if (el.matches("details")) el.open = true;
      const top = col.scrollTop + el.getBoundingClientRect().top - col.getBoundingClientRect().top - (tabs?.offsetHeight ?? 0);
      col.scrollTo({ top, behavior: "smooth" });
    });
    return b;
  }));
}

// Contained: block heights set by dragging the line between two blocks.
const blockHeights = store.get("section-heights") ?? {};

function addBlockDividers(col) {
  const blocks = columnBlocks(col);
  for (const b of blocks) {
    const h = b.id && blockHeights[b.id];
    b.style.height = h && !b.classList.contains("presets-block") ? `${h}px` : "";
    b.classList.toggle("sized", Boolean(b.style.height));
  }
  for (let i = 1; i < blocks.length; i++) {
    const above = blocks[i - 1];
    const below = blocks[i];
    // Resize the block that doesn't fill the spare height.
    const target = below.classList.contains("presets-block") || !below.id ? above : below;
    if (!target.id) continue;
    const sign = target === above ? 1 : -1;
    const grip = Object.assign(document.createElement("div"), { className: "section-resizer", title: "Drag to resize; double-click to reset" });
    below.before(grip);
    grip.addEventListener("pointerdown", (e) => {
      if (document.body.classList.contains("editing")) return;
      e.preventDefault();
      grip.setPointerCapture(e.pointerId);
      const y0 = e.clientY;
      const h0 = target.getBoundingClientRect().height;
      const move = (ev) => {
        const h = Math.max(40, Math.round(h0 + sign * (ev.clientY - y0)));
        target.style.height = `${h}px`;
        target.classList.add("sized");
        blockHeights[target.id] = h;
      };
      grip.addEventListener("pointermove", move);
      grip.addEventListener("pointerup", () => {
        grip.removeEventListener("pointermove", move);
        store.set("section-heights", blockHeights);
      }, { once: true });
    });
    grip.addEventListener("dblclick", () => {
      delete blockHeights[target.id];
      target.style.height = "";
      target.classList.remove("sized");
      store.set("section-heights", blockHeights);
    });
  }
}

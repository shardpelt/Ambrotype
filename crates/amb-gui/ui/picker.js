// SPDX-License-Identifier: GPL-3.0-or-later
// A drop-down with collapsible groups, layered over a native <select>: the
// select stays in the page (hidden) and keeps the value, so code that reads or
// sets `select.value` or listens for "change" keeps working. Used for the
// long size lists (crop aspect, export size).
//
//   AmbPicker.enhance(select, { open: ["Common", "Ratios"] });
//
// Groups named in `open` start expanded; the others are collapsed until
// clicked. The group holding the current choice always opens. Which groups
// are open is remembered for the session.
"use strict";

const AmbPicker = (() => {
  const valueProp = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value");

  function enhance(select, opts = {}) {
    const openGroups = new Set(opts.open ?? []);
    const closedGroups = new Set(); // closed by hand, even if they hold the choice
    const wrap = document.createElement("div");
    wrap.className = "picker";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "picker-button";
    button.setAttribute("aria-haspopup", "listbox");
    const label = document.createElement("span");
    label.className = "picker-label";
    button.append(label);
    const pop = document.createElement("div");
    pop.className = "picker-pop";
    pop.setAttribute("role", "listbox");
    pop.hidden = true;
    select.after(wrap);
    wrap.append(button, pop);
    select.hidden = true;
    select.tabIndex = -1;

    const selectedText = () => {
      const o = select.options[select.selectedIndex];
      return o ? o.textContent : "";
    };
    const sync = () => {
      label.textContent = selectedText();
      button.disabled = select.disabled;
    };

    // Setting the value from code updates the button too.
    Object.defineProperty(select, "value", {
      configurable: true,
      get() { return valueProp.get.call(this); },
      set(v) { valueProp.set.call(this, v); sync(); },
    });
    new MutationObserver(sync).observe(select, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled"] });

    function choose(value) {
      close();
      if (value === select.value) return;
      select.value = value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }

    function item(o) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "picker-item";
      b.setAttribute("role", "option");
      b.textContent = o.textContent;
      b.dataset.value = o.value;
      const on = o.value === select.value;
      b.classList.toggle("selected", on);
      b.setAttribute("aria-selected", String(on));
      b.addEventListener("click", () => choose(o.value));
      return b;
    }

    function render() {
      pop.replaceChildren();
      for (const child of select.children) {
        if (child.tagName === "OPTION") {
          pop.append(item(child));
          continue;
        }
        const name = child.label;
        const options = [...child.children];
        const holdsChoice = options.some((o) => o.value === select.value);
        const shown = (openGroups.has(name) || holdsChoice) && !closedGroups.has(name);
        const head = document.createElement("button");
        head.type = "button";
        head.className = "picker-group";
        head.setAttribute("aria-expanded", String(shown));
        head.append(
          Object.assign(document.createElement("span"), { className: "caret", textContent: shown ? "▾" : "▸" }),
          document.createTextNode(name),
          Object.assign(document.createElement("span"), { className: "count", textContent: options.length }),
        );
        head.addEventListener("click", () => {
          if (shown) { openGroups.delete(name); closedGroups.add(name); }
          else { openGroups.add(name); closedGroups.delete(name); }
          render();
        });
        const body = document.createElement("div");
        body.className = "picker-group-body";
        if (shown) body.append(...options.map(item));
        pop.append(head, body);
      }
    }

    function open() {
      if (select.disabled) return;
      render();
      pop.hidden = false;
      wrap.classList.add("open");
      // Fixed to the window (so a dialog's edge doesn't cut it off), below the
      // button, or above it when there is more room there.
      const r = button.getBoundingClientRect();
      const below = window.innerHeight - r.bottom - 12;
      const up = below < 260 && r.top > below;
      const room = Math.max(160, Math.min(440, up ? r.top - 12 : below));
      const width = Math.max(r.width, 320);
      Object.assign(pop.style, {
        left: `${Math.max(8, Math.min(r.left, window.innerWidth - width - 8))}px`,
        width: `${width}px`,
        maxHeight: `${room}px`,
        top: up ? "" : `${r.bottom + 4}px`,
        bottom: up ? `${window.innerHeight - r.top + 4}px` : "",
      });
      pop.querySelector(".selected")?.scrollIntoView({ block: "nearest" });
    }
    function close() {
      pop.hidden = true;
      wrap.classList.remove("open");
    }

    button.addEventListener("click", () => (pop.hidden ? open() : close()));
    button.addEventListener("keydown", (e) => {
      if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
        if (pop.hidden) { e.preventDefault(); open(); }
      }
    });
    pop.addEventListener("keydown", (e) => {
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); button.focus(); }
    });
    document.addEventListener("pointerdown", (e) => { if (!wrap.contains(e.target)) close(); }, true);
    // A fixed list would drift away from its button when something scrolls.
    document.addEventListener("scroll", (e) => { if (!pop.contains(e.target)) close(); }, true);
    window.addEventListener("resize", close);
    sync();
    return { refresh: sync, open, close };
  }

  return { enhance };
})();

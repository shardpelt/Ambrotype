// SPDX-License-Identifier: GPL-3.0-or-later
// The hub: browse shared layouts, themes and plugins, install them, and use
// them (src/hub.rs fetches, checks and saves them). Previews are drawn here from
// each look's own data (theme colours, layout columns), so the hub needs no
// screenshots and nothing is loaded from other sites into the page.
"use strict";

// A tiny mock of the editor in a theme's colours. The colours were checked
// to be plain hex values before they got here.
function themePreview(data) {
  const t = data?.tokens ?? {};
  const c = (name, fallback) => (typeof t[name] === "string" ? t[name] : fallback);
  const box = document.createElement("div");
  box.className = "hub-preview theme";
  box.style.background = c("bg", "#1b1c1f");
  box.style.borderColor = c("line", "#34363c");
  const side = document.createElement("div");
  side.className = "hp-side";
  side.style.background = c("panel", "#232428");
  for (const [w, colour] of [[70, c("text", "#e6e6e8")], [50, c("muted", "#9a9ca3")], [60, c("faint", "#6c6f77")]]) {
    const line = document.createElement("i");
    line.style.cssText = `width:${w}%;background:${colour}`;
    side.append(line);
  }
  const slider = document.createElement("b");
  slider.style.background = c("track", "#4a4d55");
  const thumb = document.createElement("u");
  thumb.style.background = c("thumb", "#d8d9dc");
  slider.append(thumb);
  side.append(slider);
  const view = document.createElement("div");
  view.className = "hp-view";
  view.style.background = c("viewer", c("bg-deep", "#111214"));
  const button = document.createElement("span");
  button.className = "hp-button";
  button.style.cssText = `background:${c("accent", "#e8b05c")};color:${c("accent-ink", "#1b1c1f")}`;
  button.textContent = "Aa";
  view.append(button);
  box.append(view, side);
  return box;
}

// A sketch of a layout: its columns with their panels and blocks, and the photo between.
function layoutPreview(data) {
  const box = document.createElement("div");
  box.className = "hub-preview layout";
  const name = (item) => {
    if (typeof item === "string") return BLOCK_TITLES[item] ?? item;
    return item.title ?? schemaPanel(item.panel)?.label ?? "Panel";
  };
  const column = (side) => {
    const spec = data?.[side] ?? {};
    const items = Array.isArray(spec.items) ? spec.items : [];
    const width = Number(spec.width ?? DEFAULT_WIDTH[side]);
    if (!items.length || !(width > 0)) return null;
    const col = document.createElement("div");
    col.className = "hp-col";
    col.style.flex = `0 0 ${Math.round((width / (width + 600)) * 100)}%`;
    for (const item of items.slice(0, 9)) {
      const bar = document.createElement("i");
      bar.textContent = name(item);
      if (typeof item === "object" && item.columns > 1) bar.classList.add("grid");
      if (typeof item === "string") bar.classList.add("block");
      col.append(bar);
    }
    if (items.length > 9) col.append(Object.assign(document.createElement("i"), { textContent: `+${items.length - 9} more` }));
    return col;
  };
  const view = document.createElement("div");
  view.className = "hp-photo";
  box.append(...[column("left"), view, column("right")].filter(Boolean));
  return box;
}

// What a plugin asks for and what it offers, from its manifest.
const PLUGIN_PERMISSIONS = {
  "photo-info": "the camera, lens and exposure",
  "photo-stats": "the photo's histogram",
};
function pluginPreview(data) {
  const box = document.createElement("div");
  box.className = "hub-preview plugin";
  const perms = (data?.permissions ?? []).map((p) => PLUGIN_PERMISSIONS[p] ?? p);
  const sees = Object.assign(document.createElement("span"), {
    textContent: perms.length ? `Asks to see ${perms.join(" and ")}` : "Sees only the edit's settings",
  });
  const params = (data?.params ?? []).map((p) => p.label ?? p.name).filter(Boolean);
  const offers = Object.assign(document.createElement("span"), {
    textContent: params.length ? `Options: ${params.slice(0, 4).join(", ")}${params.length > 4 ? ", …" : ""}` : "No options",
  });
  const sandbox = Object.assign(document.createElement("b"), { textContent: "Runs sandboxed; its edits are undoable" });
  box.append(sandbox, sees, offers);
  return box;
}

// What a develop preset changes (the backend read and checked it).
function presetPreview(data) {
  const box = document.createElement("div");
  box.className = "hub-preview preset";
  const count = Object.assign(document.createElement("b"), {
    textContent: `${data?.settings ?? 0} setting${data?.settings === 1 ? "" : "s"}`,
  });
  const groups = Object.assign(document.createElement("span"), { textContent: (data?.groups ?? []).join(", ") });
  box.append(count, groups);
  if (data?.group) box.append(Object.assign(document.createElement("span"), { textContent: `In “${data.group}”` }));
  return box;
}

// A collage template's cells, drawn to scale inside its page.
function collagePreview(data) {
  const box = document.createElement("div");
  box.className = "hub-preview collage";
  const ratio = Number(data?.width) / Number(data?.height) || 1;
  // Fit 220 × 104 (the card), keeping the page's shape.
  const width = Math.min(220, 104 * ratio);
  box.style.width = `${Math.round(width)}px`;
  box.style.height = `${Math.round(width / ratio)}px`;
  box.style.background = typeof data?.background === "string" ? data.background : "#ffffff";
  for (const c of (data?.cells ?? []).slice(0, 64)) {
    const [x, y, w, h, angle] = c.map(Number);
    const cell = document.createElement("i");
    cell.style.cssText = `left:calc(${x * 100}% + 2px);top:calc(${y * 100}% + 2px);` +
      `width:calc(${w * 100}% - 4px);height:calc(${h * 100}% - 4px)`;
    if (angle) cell.style.transform = `rotate(${angle}deg)`;
    box.append(cell);
  }
  return box;
}

// A size profile's sizes.
function sizesPreview(data) {
  const box = document.createElement("div");
  box.className = "hub-preview sizes";
  const sizes = data?.sizes ?? [];
  box.append(Object.assign(document.createElement("b"), { textContent: `${sizes.length} size${sizes.length === 1 ? "" : "s"}` }));
  for (const s of sizes.slice(0, 4)) {
    box.append(Object.assign(document.createElement("span"), { textContent: `${s.name}: ${s.width} × ${s.height}` }));
  }
  if (sizes.length > 4) box.append(Object.assign(document.createElement("span"), { textContent: "…" }));
  return box;
}

async function hubFlow(initial = "all") {
  const dlg = $("hub-dialog");
  let kind = initial;
  let list = null;
  const status = (text, error = false) => {
    $("hub-status").textContent = text;
    $("hub-status").classList.toggle("error", error);
  };

  function render() {
    const grid = $("hub-grid");
    grid.replaceChildren();
    if (!list) return;
    const filter = $("hub-search").value.trim().toLowerCase();
    const shown = list.entries.filter((e) => (kind === "all" || e.kind === kind) &&
      (!filter || `${e.name} ${e.author} ${e.description}`.toLowerCase().includes(filter)));
    for (const e of shown) {
      const card = document.createElement("article");
      card.className = "hub-card";
      if (e.data) {
        const preview = {
          theme: themePreview, layout: layoutPreview, plugin: pluginPreview, preset: presetPreview,
          collage: collagePreview, sizes: sizesPreview,
        }[e.kind];
        if (preview) card.append(preview(e.data));
      }
      const title = document.createElement("h3");
      title.textContent = e.name;
      const kindTag = Object.assign(document.createElement("span"), { className: "hub-kind", textContent: e.kind });
      title.append(kindTag);
      const by = Object.assign(document.createElement("p"), { className: "hint", textContent: [e.author && `by ${e.author}`, e.license].filter(Boolean).join(" · ") });
      const about = Object.assign(document.createElement("p"), { className: "hub-about", textContent: e.description });
      card.append(title, by, about);
      if (e.error) {
        card.append(Object.assign(document.createElement("p"), { className: "hub-error", textContent: `Can't be installed: ${e.error}` }));
      } else {
        const actions = document.createElement("div");
        actions.className = "hub-actions";
        const install = Object.assign(document.createElement("button"), { type: "button" });
        if (e.installed && !e.update) {
          install.textContent = "Installed ✓";
          install.disabled = true;
        } else {
          install.textContent = e.update ? "Update" : "Install";
          install.className = "primary";
          install.addEventListener("click", async () => {
            install.disabled = true;
            install.textContent = "Installing…";
            try {
              if (e.kind === "collage") {
                await invoke("hub_install_collage", { id: e.id });
                e.installed = e.id;
                status(`Installed "${e.name}": it's with the templates in the collage maker.`);
              } else if (e.kind === "sizes") {
                ui.sizes = await invoke("hub_install_sizes", { id: e.id });
                e.installed = e.id;
                status(`Installed "${e.name}": its sizes are in the crop tool and the export dialog.`);
              } else if (e.kind === "preset") {
                ui.presets = await invoke("hub_install_preset", { id: e.id });
                renderPresets();
                e.installed = e.id;
                status(`Installed "${e.name}": it's with the presets on the left.`);
              } else if (e.kind === "plugin") {
                e.installed = await invoke("hub_install_plugin", { id: e.id });
                status(`Installed "${e.name}": it's in the Plugins dialog (Ctrl+Shift+P), and asks before it first runs.`);
              } else {
                const r = await invoke("hub_install", { id: e.id });
                ui.looks = r.looks;
                e.installed = r.id;
                status(`Installed "${e.name}".`);
              }
              e.update = false;
            } catch (err) {
              status(String(err), true);
            }
            render();
          });
        }
        actions.append(install);
        if (e.installed && (e.kind === "layout" || e.kind === "theme")) {
          const use = Object.assign(document.createElement("button"), { type: "button", textContent: "Use now" });
          use.addEventListener("click", () => {
            const field = e.kind === "layout" ? "active_layout" : "active_theme";
            ui.looks[field] = e.installed;
            applyActiveLooks();
            invoke("set_active_look", { layout: ui.looks.active_layout, theme: ui.looks.active_theme }).catch((err) => toast(String(err), true));
            status(`Using "${e.name}".`);
          });
          actions.append(use);
        }
        card.append(actions);
      }
      grid.append(card);
    }
    if (!shown.length) grid.append(Object.assign(document.createElement("p"), { className: "hint", textContent: "Nothing matches." }));
  }

  const tabs = [...dlg.querySelectorAll(".hub-tabs button")];
  const onTab = (ev) => {
    kind = ev.currentTarget.dataset.kind;
    tabs.forEach((t) => t.classList.toggle("active", t === ev.currentTarget));
    render();
  };
  tabs.forEach((t) => t.classList.toggle("active", t.dataset.kind === kind));
  tabs.forEach((t) => t.addEventListener("click", onTab));
  $("hub-search").addEventListener("input", render);
  $("hub-search").value = "";
  $("hub-grid").replaceChildren();
  status("Loading the hub…");
  dlg.showModal();
  invoke("hub_browse")
    .then((r) => {
      list = r;
      $("hub-address").textContent = r.url;
      const count = (k) => r.entries.filter((e) => e.kind === k).length;
      const parts = [[count("layout"), "layout"], [count("theme"), "theme"], [count("preset"), "preset"],
        [count("collage"), "collage template"], [count("sizes"), "size profile"], [count("plugin"), "plugin"]]
        .filter(([n]) => n).map(([n, w]) => `${n} ${w}${n === 1 ? "" : "s"}`);
      status(parts.length ? `${parts.join(", ")}.` : "The hub is empty so far.");
      render();
    })
    .catch(async (err) => {
      $("hub-address").textContent = await invoke("hub_url").catch(() => "");
      status(`The hub can't be reached: ${err}. Check your connection, or the hub address in Settings → General.`, true);
    });
  await new Promise((resolve) => dlg.addEventListener("close", resolve, { once: true }));
  tabs.forEach((t) => t.removeEventListener("click", onTab));
  $("hub-search").removeEventListener("input", render);
}

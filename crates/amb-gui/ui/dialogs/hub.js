// SPDX-License-Identifier: GPL-3.0-or-later
// The hub: browse shared layouts, themes and plugins, install them, and use
// them (src/hub/ fetches, checks and saves them). Previews are drawn here from
// each look's own data (theme colours, layout columns), so the hub needs no
// screenshots and nothing is loaded from other sites into the page.
"use strict";

// What the card's tag says (the kinds are ids from the hub's index).
const HUB_KINDS = {
  layout: tKey("layout"), theme: tKey("theme"), preset: tKey("preset"), collage: tKey("collage"), sizes: tKey("sizes"),
  border: tKey("border"), plugin: tKey("plugin"), interface: tKey("interface"),
};

// A tiny mock of the editor in a theme's colours. The colours were checked
// to be plain hex values before they got here.
function themePreview(data) {
  const t = data?.tokens ?? {};
  const c = (name, fallback) => (typeof t[name] === "string" ? t[name] : fallback);
  const box = el("div", { class: "hub-preview theme" });
  box.style.background = c("bg", "#1b1c1f");
  box.style.borderColor = c("line", "#34363c");
  const side = el("div", { class: "hp-side" });
  side.style.background = c("panel", "#232428");
  for (const [w, colour] of [[70, c("text", "#e6e6e8")], [50, c("muted", "#9a9ca3")], [60, c("faint", "#6c6f77")]]) {
    const line = el("i");
    line.style.cssText = `width:${w}%;background:${colour}`;
    side.append(line);
  }
  const slider = el("b");
  slider.style.background = c("track", "#4a4d55");
  const thumb = el("u");
  thumb.style.background = c("thumb", "#d8d9dc");
  slider.append(thumb);
  side.append(slider);
  const view = el("div", { class: "hp-view" });
  view.style.background = c("viewer", c("bg-deep", "#111214"));
  const button = el("span", { class: "hp-button" });
  button.style.cssText = `background:${c("accent", "#e8b05c")};color:${c("accent-ink", "#1b1c1f")}`;
  button.textContent = "Aa";
  view.append(button);
  box.append(view, side);
  return box;
}

// A sketch of a layout: its columns with their panels and blocks, and the photo between.
function layoutPreview(data) {
  const box = el("div", { class: "hub-preview layout" });
  const name = (item) => {
    if (typeof item === "string") return t(BLOCK_TITLES[item] ?? item);
    // A layout's own title is its author's word; a panel's name is ours.
    return item.title ?? t(schemaPanel(item.panel)?.label ?? "Panel");
  };
  const column = (side) => {
    const spec = data?.[side] ?? {};
    const items = Array.isArray(spec.items) ? spec.items : [];
    const width = Number(spec.width ?? DEFAULT_WIDTH[side]);
    if (!items.length || !(width > 0)) return null;
    const col = el("div", { class: "hp-col" });
    col.style.flex = `0 0 ${Math.round((width / (width + 600)) * 100)}%`;
    for (const item of items.slice(0, 9)) {
      const bar = el("i", {}, name(item));
      if (typeof item === "object" && item.columns > 1) bar.classList.add("grid");
      if (typeof item === "string") bar.classList.add("block");
      col.append(bar);
    }
    if (items.length > 9) col.append(el("i", {}, t("+{n} more", { n: items.length - 9 })));
    return col;
  };
  const view = el("div", { class: "hp-photo" });
  box.append(...[column("left"), view, column("right")].filter(Boolean));
  return box;
}

// What a plugin asks for and what it offers, from its manifest.
const PLUGIN_PERMISSIONS = {
  "photo-info": tKey("the camera, lens and exposure"),
  "photo-stats": tKey("the photo's histogram"),
};
function pluginPreview(data) {
  const box = el("div", { class: "hub-preview plugin" });
  const perms = (data?.permissions ?? []).map((p) => t(PLUGIN_PERMISSIONS[p] ?? p));
  const sees = el("span", {}, perms.length ? t("Asks to see {what}", { what: perms.reduce((a, b) => t("{a} and {b}", { a, b })) }) : t("Sees only the edit's settings"));
  const params = (data?.params ?? []).map((p) => p.label ?? p.name).filter(Boolean);
  const offers = el("span", {}, params.length ? t("Options: {list}", { list: `${params.slice(0, 4).join(", ")}${params.length > 4 ? ", …" : ""}` }) : t("No options"));
  const sandbox = el("b", {}, t("Runs sandboxed; its edits are undoable"));
  box.append(sandbox, sees, offers);
  return box;
}

// A custom interface: code with the app's full reach, so say so, and what
// it's made of.
function interfacePreview(data) {
  const box = el("div", { class: "hub-preview interface" });
  const files = data?.files ?? [];
  box.append(
    el("b", {}, t("A whole other interface: code with the app's full reach")),
    el("span", {}, t("{files}: {list}", { files: plural(files.length, "file"), list: `${files.slice(0, 4).join(", ")}${files.length > 4 ? ", …" : ""}` })),
    el("span", {}, t("Read its files in the hub repo before installing")),
  );
  return box;
}

// What a develop preset changes (the backend read and checked it).
function presetPreview(data) {
  const box = el("div", { class: "hub-preview preset" });
  const count = el("b", {}, plural(data?.settings ?? 0, "setting"));
  const groups = el("span", { translate: "no" }, (data?.groups ?? []).join(", "));
  box.append(count, groups);
  if (data?.group) box.append(el("span", {}, t("In “{group}”", { group: data.group })));
  return box;
}

// A collage template's cells, drawn to scale inside its page.
function collagePreview(data) {
  const box = el("div", { class: "hub-preview collage" });
  const ratio = Number(data?.width) / Number(data?.height) || 1;
  // Fit 220 × 104 (the card), keeping the page's shape.
  const width = Math.min(220, 104 * ratio);
  box.style.width = `${Math.round(width)}px`;
  box.style.height = `${Math.round(width / ratio)}px`;
  box.style.background = typeof data?.background === "string" ? data.background : "#ffffff";
  for (const c of (data?.cells ?? []).slice(0, 64)) {
    const [x, y, w, h, angle] = c.map(Number);
    const cell = el("i");
    cell.style.cssText = `left:calc(${x * 100}% + 2px);top:calc(${y * 100}% + 2px);` +
      `width:calc(${w * 100}% - 4px);height:calc(${h * 100}% - 4px)`;
    if (angle) cell.style.transform = `rotate(${angle}deg)`;
    box.append(cell);
  }
  return box;
}

// A border around a grey photo, to scale (a 3:2 photo).
function borderPreview(data) {
  const box = el("div", { class: "hub-preview border" });
  const pct = (v) => (data?.unit === "px" ? 0 : Math.max(0, Number(v) || 0));
  const side = pct(data?.width);
  const bottom = data?.bottom == null ? side : pct(data.bottom);
  // Percent of the short edge (64 px tall photo).
  const px = (v) => Math.round((v / 100) * 64);
  const frame = el("div");
  frame.style.cssText = `padding:${px(side)}px ${px(side)}px ${px(bottom)}px;border-radius:${px(Number(data?.radius) || 0)}px`;
  frame.style.background = /^#[0-9a-f]{3,8}$/i.test(data?.color ?? "") ? data.color : "#888";
  const photo = el("i");
  const key = data?.keyline;
  if (key?.width > 0) photo.style.outline = `${Math.max(1, px(key.width))}px solid ${key.color}`;
  frame.append(photo);
  box.append(frame);
  return box;
}

// A size profile's sizes.
function sizesPreview(data) {
  const box = el("div", { class: "hub-preview sizes" });
  const sizes = data?.sizes ?? [];
  box.append(el("b", {}, plural(sizes.length, "size")));
  for (const s of sizes.slice(0, 4)) {
    box.append(el("span", { translate: "no" }, `${s.name}: ${s.width} × ${s.height}`));
  }
  if (sizes.length > 4) box.append(el("span", {}, "…"));
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
      const card = el("article", { class: "hub-card" });
      if (e.data) {
        const preview = {
          theme: themePreview, layout: layoutPreview, plugin: pluginPreview, preset: presetPreview,
          collage: collagePreview, sizes: sizesPreview, border: borderPreview, interface: interfacePreview,
        }[e.kind];
        if (preview) card.append(preview(e.data));
      }
      const kindTag = el("span", { class: "hub-kind" }, t(HUB_KINDS[e.kind] ?? e.kind));
      const title = el("h3", {}, el("span", { translate: "no" }, e.name), kindTag);
      const by = el("p", { class: "hint" }, [e.author && t("by {author}", { author: e.author }), e.license].filter(Boolean).join(" · "));
      const about = el("p", { class: "hub-about", translate: "no" }, e.description);
      card.append(title, by, about);
      if (e.error) {
        card.append(el("p", { class: "hub-error" }, t("Can't be installed: {error}", { error: e.error })));
      } else {
        const actions = el("div", { class: "hub-actions" });
        const install = el("button", { type: "button" });
        if (e.installed && !e.update) {
          install.textContent = e.installed.startsWith("builtin:") ? t("Built in ✓") : t("Installed ✓");
          install.disabled = true;
        } else {
          install.textContent = e.update ? t("Update") : t("Install");
          install.className = "primary";
          install.addEventListener("click", async () => {
            install.disabled = true;
            install.textContent = t("Installing…");
            try {
              if (e.kind === "collage") {
                await invoke("hub_install_collage", { id: e.id });
                e.installed = e.id;
                status(t('Installed "{name}": it\'s with the templates in the collage maker.', { name: e.name }));
              } else if (e.kind === "border") {
                ui.borders = await invoke("hub_install_border", { id: e.id });
                e.installed = e.id;
                status(t('Installed "{name}": it\'s with the borders in the export dialog and the Frame panel.', { name: e.name }));
              } else if (e.kind === "sizes") {
                ui.sizes = await invoke("hub_install_sizes", { id: e.id });
                e.installed = e.id;
                status(t('Installed "{name}": its sizes are in the crop tool and the export dialog.', { name: e.name }));
              } else if (e.kind === "preset") {
                ui.presets = await invoke("hub_install_preset", { id: e.id });
                renderPresets();
                e.installed = e.id;
                status(t('Installed "{name}": it\'s with the presets on the left.', { name: e.name }));
              } else if (e.kind === "interface") {
                if (!await askYes(t('Install the interface "{name}"?\n\nIt\'s a program with the app\'s full reach (your photos, edits and settings), not a sandboxed plugin. Install it only if you trust its author or have read its files on the hub.', { name: e.name }))) {
                  render();
                  return;
                }
                e.installed = await invoke("hub_install_interface", { id: e.id });
                status(t('Installed "{name}": choose it in Settings → General → Interface, or press Use now.', { name: e.name }));
              } else if (e.kind === "plugin") {
                e.installed = await invoke("hub_install_plugin", { id: e.id });
                status(t('Installed "{name}": it\'s in the Plugins dialog (Ctrl+Shift+P), and asks before it first runs.', { name: e.name }));
              } else {
                const r = await invoke("hub_install", { id: e.id });
                ui.looks = r.looks;
                e.installed = r.id;
                status(t('Installed "{name}".', { name: e.name }));
              }
              e.update = false;
            } catch (err) {
              status(String(err), true);
            }
            render();
          });
        }
        actions.append(install);
        if (e.installed && e.kind === "interface") {
          const use = el("button", { type: "button" }, t("Use now"));
          // Installed shows the folder's name; switching wants its path.
          use.addEventListener("click", async () => {
            const { installed } = await invoke("interfaces");
            const hit = installed.find((i) => i.path === e.installed || fileName(i.path) === e.installed);
            if (hit) useInterface(hit.path);
          });
          actions.append(use);
        }
        if (e.installed && (e.kind === "layout" || e.kind === "theme")) {
          const use = el("button", { type: "button" }, t("Use now"));
          use.addEventListener("click", () => {
            const field = e.kind === "layout" ? "active_layout" : "active_theme";
            ui.looks[field] = e.installed;
            applyActiveLooks();
            invoke("set_active_look", { layout: ui.looks.active_layout, theme: ui.looks.active_theme }).catch(showError);
            status(t('Using "{name}".', { name: e.name }));
          });
          actions.append(use);
        }
        card.append(actions);
      }
      grid.append(card);
    }
    if (!shown.length) grid.append(el("p", { class: "hint" }, t("Nothing matches.")));
  }

  // A look shared by link (a file, or a GitHub repo with theme.json or layout.json).
  const onLink = async () => {
    const link = $("hub-link").value.trim();
    if (!link) return $("hub-link").focus();
    $("hub-link-install").disabled = true;
    status(t("Installing from the link…"));
    try {
      const r = await invoke("hub_install_link", { link });
      ui.looks = r.looks;
      ui.looks[r.kind === "layout" ? "active_layout" : "active_theme"] = r.id;
      applyActiveLooks();
      await invoke("set_active_look", { layout: ui.looks.active_layout, theme: ui.looks.active_theme });
      $("hub-link").value = "";
      status(t('Installed and using "{name}". Appearance (Ctrl+L) switches back.', { name: findLook(r.kind, r.id)?.name ?? r.id }));
    } catch (err) {
      status(String(err), true);
    }
    $("hub-link-install").disabled = false;
  };
  const onLinkKey = (ev) => { if (ev.key === "Enter") { ev.preventDefault(); onLink(); } };
  $("hub-link-install").addEventListener("click", onLink);
  $("hub-link").addEventListener("keydown", onLinkKey);

  const tabs = [...dlg.querySelectorAll(".hub-tabs button")];
  const onTab = (ev) => {
    kind = ev.currentTarget.dataset.kind;
    tabs.forEach((tab) => tab.classList.toggle("active", tab === ev.currentTarget));
    render();
  };
  tabs.forEach((tab) => tab.classList.toggle("active", tab.dataset.kind === kind));
  tabs.forEach((tab) => tab.addEventListener("click", onTab));
  $("hub-search").addEventListener("input", render);
  $("hub-search").value = "";
  boxTip($("hub-search"));
  $("hub-grid").replaceChildren();
  status(t("Loading the hub…"));
  dlg.showModal();
  invoke("hub_browse")
    .then((r) => {
      list = r;
      $("hub-address").textContent = r.url;
      const count = (k) => r.entries.filter((e) => e.kind === k).length;
      const has = (k, say) => (count(k) ? say(count(k)) : null);
      const parts = [
        has("layout", (n) => plural(n, "layout")), has("theme", (n) => plural(n, "theme")), has("preset", (n) => plural(n, "preset")),
        has("collage", (n) => plural(n, "collage template")), has("sizes", (n) => plural(n, "size profile")),
        has("border", (n) => plural(n, "border")), has("plugin", (n) => plural(n, "plugin")), has("interface", (n) => plural(n, "interface")),
      ].filter(Boolean);
      status(parts.length ? `${parts.join(", ")}.` : t("The hub is empty so far."));
      render();
    })
    .catch(async (err) => {
      $("hub-address").textContent = await invoke("hub_url").catch(() => "");
      status(t("The hub can't be reached: {error}. Check your connection, or the hub address in Settings → General.", { error: err }), true);
    });
  await new Promise((resolve) => dlg.addEventListener("close", resolve, { once: true }));
  tabs.forEach((tab) => tab.removeEventListener("click", onTab));
  $("hub-search").removeEventListener("input", render);
  $("hub-link-install").removeEventListener("click", onLink);
  $("hub-link").removeEventListener("keydown", onLinkKey);
}

// The toolbar's Browse button: the whole hub, then pick up what was installed.
async function browseFlow() {
  await hubFlow();
  if (typeof loadPlugins === "function") await loadPlugins().catch(() => {});
}

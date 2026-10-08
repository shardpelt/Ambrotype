// The hub: browse shared layouts and themes, install them, and use them
// (src/hub.rs fetches, checks and saves them). Previews are drawn here from
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

async function hubFlow() {
  const dlg = $("hub-dialog");
  let kind = "all";
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
      if (e.data) card.append(e.kind === "theme" ? themePreview(e.data) : layoutPreview(e.data));
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
              const r = await invoke("hub_install", { id: e.id });
              ui.looks = r.looks;
              e.installed = r.id;
              e.update = false;
              status(`Installed "${e.name}".`);
            } catch (err) {
              status(String(err), true);
            }
            render();
          });
        }
        actions.append(install);
        if (e.installed) {
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
      status(r.entries.length ? `${r.entries.length} layouts and themes.` : "The hub is empty so far.");
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

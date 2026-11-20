// SPDX-License-Identifier: GPL-3.0-or-later
// The Appearance dialog: layouts, themes and the toolbar's order. Split out of
// layout.js.
"use strict";

// ---------------------------------------------------------------- Appearance dialog

// The functions below take the dialog's state `lk` (made in looksFlow):
// `before` is the layout and theme it opened with, `sel` the ones chosen now,
// `view` the look the name, clone, share and delete row acts on, `armedDelete`
// the look whose Delete was pressed once, `setups` the saved setups.

// The name of the setup kept before each change, so one click brings it back.
const LOOKS_PREVIOUS = "Before the last change";

const looksViewed = (lk) => findLook(lk.view.kind, lk.view.id);

function looksApply(lk) {
  const l = findLook("layout", lk.sel.layout), t = findLook("theme", lk.sel.theme);
  applyTheme(t?.usable ? t.data : null);
  if (l?.usable) applyLayout(l.data);
}

function looksChoose(lk, kind, look) {
  lk.view = { kind, id: look.id };
  if (look.usable) {
    lk.sel[kind] = look.id;
    // A layout may name the theme it was made with.
    const t = kind === "layout" && look.data.theme && findLook("theme", look.data.theme);
    if (t?.usable) lk.sel.theme = t.id;
    looksApply(lk);
  }
  looksRender(lk);
}

function looksSaved(lk, r) {
  ui.looks = r.looks;
  const look = findLook(r.kind, r.id);
  if (look) looksChoose(lk, r.kind, look);
}

// Saved setups: a layout and theme together, kept as they were.
function looksRenderSetups(lk) {
  const list = $("looks-setups-list");
  if (!lk.setups.length) {
    list.replaceChildren(el("p", { class: "hint" }, t("None yet.")));
    return;
  }
  list.replaceChildren(...lk.setups.map((s) => {
    const row = document.createElement("div");
    row.className = "looks-setup";
    const when = new Date(s.saved * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
    const name = el("span", {
      title: t("{layout} + {theme}, kept {when}", { layout: s.layout.data?.name ?? s.layout.id, theme: s.theme.data?.name ?? s.theme.id, when }),
      translate: "no",
    }, s.name);
    const what = el("span", {
      class: "hint",
      translate: "no",
    }, `${s.layout.data?.name ?? "?"} · ${s.theme.data?.name ?? "?"}`);
    const use = el("button", { type: "button", class: "small" }, t("Use"));
    use.addEventListener("click", async () => {
      try {
        const r = await invoke("setup_restore", { name: s.name });
        ui.looks = r.looks;
        lk.sel.layout = r.layout;
        lk.sel.theme = r.theme;
        lk.view = { kind: "layout", id: lk.sel.layout };
        looksApply(lk);
        looksRender(lk);
        toast(t('"{name}" is back: Done keeps it', { name: s.name }));
      } catch (e) {
        showError(e);
      }
    });
    const del = el("button", { type: "button", class: "small", title: t("Forget this setup (its layout and theme stay)") }, "×");
    del.addEventListener("click", async () => {
      lk.setups = await invoke("setup_delete", { name: s.name }).catch((e) => { showError(e); return lk.setups; });
      looksRenderSetups(lk);
    });
    row.append(name, what, use, del);
    return row;
  }));
}

async function looksKeepSetup(lk) {
  const layout = findLook("layout", lk.sel.layout), theme = findLook("theme", lk.sel.theme);
  const name = await askName(t("Save this setup"), t("The chosen layout and theme, exactly as they are now."),
    t("{layout} + {theme}", { layout: layout?.name ?? t("Layout"), theme: theme?.name ?? t("theme") }));
  if (!name) return;
  try {
    lk.setups = await invoke("setup_save", { name, layout: lk.sel.layout, theme: lk.sel.theme });
    looksRenderSetups(lk);
    toast(t('Saved the setup "{name}"', { name }));
  } catch (e) {
    showError(e);
  }
}

function looksRender(lk) {
  const { sel, view } = lk;
  for (const kind of ["layout", "theme"]) {
    const list = $(kind === "layout" ? "looks-layouts" : "looks-themes");
    // A task's own layout is in the task bar; it's listed only while chosen.
    const shown = ui.looks[`${kind}s`].filter((look) => !TASK_LAYOUTS.has(look.id) || sel[kind] === look.id);
    list.replaceChildren(...shown.map((look) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "look";
      b.setAttribute("role", "option");
      b.setAttribute("aria-selected", String(sel[kind] === look.id));
      b.classList.toggle("selected", sel[kind] === look.id);
      b.classList.toggle("viewed", view.kind === kind && view.id === look.id);
      b.classList.toggle("broken", !look.usable);
      const name = document.createElement("span");
      name.textContent = look.name;
      name.setAttribute("translate", "no");
      const marks = document.createElement("span");
      marks.className = "marks";
      marks.textContent = (look.problems?.length ? "⚠ " : "") + (look.builtin ? "🔒" : "");
      b.title = look.builtin ? t("Built in: clone it to make changes") : (look.usable ? "" : t("Can't be used: see the problems below"));
      b.append(name, marks);
      b.addEventListener("click", () => looksChoose(lk, kind, look));
      return b;
    }));
  }
  const look = looksViewed(lk);
  $("looks-name").value = look?.name ?? "";
  $("looks-name").disabled = !look || look.builtin || !look.usable;
  $("looks-clone").disabled = !look?.usable;
  $("looks-share").disabled = !look?.usable;
  $("looks-delete").disabled = !look || look.builtin;
  $("looks-submit").disabled = !look || look.builtin || !look.usable;
  $("looks-delete").textContent = lk.armedDelete === look?.id ? t("Really delete?") : t("Delete");
  const about = [];
  if (look?.data?.description) about.push(look.data.description);
  const base = look?.based_on && findLook(view.kind, look.based_on);
  if (look?.based_on) about.push(t("Based on {name}.", { name: base?.name ?? look.based_on }));
  if (look && !look.builtin) about.push(view.kind === "layout" ? t("Your layout: a file in your own folder (Open folder).") : t("Your theme: a file in your own folder (Open folder)."));
  $("looks-about").textContent = about.join(" ");
  $("looks-problems").replaceChildren(...(look?.problems ?? []).map((p) => {
    const li = document.createElement("li");
    li.className = p.error ? "error" : "warning";
    li.textContent = p.error ? t("Error: {message}", { message: p.message }) : t("Warning: {message}", { message: p.message });
    return li;
  }));
  looksThemeEditor(lk, view.kind === "theme" && look && !look.builtin && look.usable ? look : null);
}

// Colour pickers for one of the user's themes; changes show at once and save on release.
function looksThemeEditor(lk, look) {
  const box = $("theme-editor");
  box.hidden = !look;
  if (!look) return box.replaceChildren();
  const data = structuredClone(look.data);
  data.tokens ??= {};
  const fallback = findLook("theme", "builtin:dark")?.data.tokens ?? {};
  const save = async () => {
    try {
      looksSaved(lk, await invoke("save_look", { kind: "theme", id: look.id, data }));
    } catch (e) { showError(e); }
  };
  const rows = Object.keys(fallback).map((token) => {
    const label = document.createElement("label");
    label.className = "token";
    const value = data.tokens[token] ?? fallback[token];
    let input;
    if (typeof fallback[token] === "number") {
      input = el("input", { type: "range", min: 0, max: 16, step: 1, value: value });
    } else {
      input = el("input", { type: "color", value: toHex6(value) });
    }
    input.addEventListener("input", () => {
      data.tokens[token] = input.type === "range" ? Number(input.value) : input.value;
      applyTheme(data);
    });
    input.addEventListener("change", save);
    label.append(input, document.createTextNode(token));
    return label;
  });
  box.replaceChildren(...rows);
}

async function looksFlow() {
  const dlg = $("looks-dialog");
  if (!ui.looks) return toast(t("Layouts and themes aren't available"), true);
  const before = { layout: ui.looks.active_layout, theme: ui.looks.active_theme };
  const lk = { before, sel: { ...before }, view: { kind: "layout", id: before.layout }, armedDelete: null, setups: [] };
  const { sel } = lk;
  const render = () => looksRender(lk);
  const apply = () => looksApply(lk);
  const viewed = () => looksViewed(lk);
  const saved = (r) => looksSaved(lk, r);

  $("looks-keep").onclick = () => looksKeepSetup(lk);
  invoke("setups_list").catch(() => []).then((s) => {
    lk.setups = s;
    looksRenderSetups(lk);
  });

  const handlers = {
    "looks-clone": async () => {
      const look = viewed();
      const data = structuredClone(look.data);
      const names = new Set(ui.looks[`${lk.view.kind}s`].map((l) => l.name));
      let name = t("{name} copy", { name: look.name });
      for (let n = 2; names.has(name); n++) name = t("{name} copy {n}", { name: look.name, n });
      data.name = name;
      data.based_on = look.id;
      saved(await invoke("save_look", { kind: lk.view.kind, id: null, data }));
      $("looks-name").focus();
      $("looks-name").select();
    },
    "looks-share": async () => {
      const look = viewed();
      const path = await invoke("export_look", { kind: lk.view.kind, id: look.id, name: look.name });
      if (path) toast(t("Saved {path}", { path }));
    },
    "looks-delete": async () => {
      const look = viewed();
      if (lk.armedDelete !== look.id) {
        lk.armedDelete = look.id;
        setTimeout(() => { if (lk.armedDelete === look.id) { lk.armedDelete = null; render(); } }, 3000);
        return render();
      }
      lk.armedDelete = null;
      ui.looks = await invoke("delete_look", { kind: lk.view.kind, id: look.id });
      const fallback = lk.view.kind === "layout" ? "builtin:classic" : "builtin:dark";
      if (sel[lk.view.kind] === look.id) sel[lk.view.kind] = fallback;
      if (before[lk.view.kind] === look.id) before[lk.view.kind] = fallback;
      lk.view = { kind: lk.view.kind, id: sel[lk.view.kind] };
      apply();
      render();
    },
    "looks-import": async () => {
      const r = await invoke("import_look");
      if (r) { saved(r); toast(t('Imported "{name}"', { name: findLook(r.kind, r.id)?.name })); }
    },
    "looks-folder": async () => toast(t("Your looks: {path}", { path: await invoke("open_looks_folder") })),
    "looks-hub": async () => {
      await hubFlow();
      // Something may have been installed (or switched to with "Use now").
      ui.looks = await invoke("looks");
      sel.layout = ui.looks.active_layout;
      sel.theme = ui.looks.active_theme;
      lk.view = { kind: lk.view.kind, id: sel[lk.view.kind] };
      render();
    },
    "looks-submit": async () => {
      const look = viewed();
      await invoke("hub_submit", { kind: lk.view.kind, id: look.id });
      toast(lk.view.kind === "layout"
        ? t("GitHub is open with your layout filled in: press Propose changes, then Create pull request.")
        : t("GitHub is open with your theme filled in: press Propose changes, then Create pull request."));
    },
    "looks-edit": async () => {
      // Keep the chosen look, then edit it.
      dlg.close("ok");
      setTimeout(startEditing);
    },
    "looks-blank": async () => {
      dlg.close("ok");
      setTimeout(() => startEditing(true));
    },
    "looks-reload": async () => {
      ui.looks = await invoke("looks");
      for (const kind of ["layout", "theme"]) {
        if (!findLook(kind, sel[kind])?.usable) sel[kind] = kind === "layout" ? "builtin:classic" : "builtin:dark";
      }
      if (!viewed()) lk.view = { kind: "layout", id: sel.layout };
      apply();
      render();
    },
  };
  const wired = Object.entries(handlers).map(([id, fn]) => {
    const h = () => fn().catch(showError);
    $(id).addEventListener("click", h);
    return [id, h];
  });
  const rename = async () => {
    const look = viewed();
    const name = $("looks-name").value.trim();
    if (!look || look.builtin || !name || name === look.name) return render();
    try {
      saved(await invoke("save_look", { kind: lk.view.kind, id: look.id, data: { ...look.data, name } }));
    } catch (e) { showError(e); }
  };
  $("looks-name").addEventListener("change", rename);
  // Enter would submit the form (as Cancel); make it confirm the name instead.
  const enter = (e) => { if (e.key === "Enter") { e.preventDefault(); $("looks-name").blur(); } };
  $("looks-name").addEventListener("keydown", enter);

  render();
  dlg.returnValue = "";
  dlg.showModal();
  await new Promise((resolve) => dlg.addEventListener("close", resolve, { once: true }));
  for (const [id, h] of wired) $(id).removeEventListener("click", h);
  $("looks-name").removeEventListener("change", rename);
  $("looks-name").removeEventListener("keydown", enter);

  if (dlg.returnValue === "ok") {
    // Keep what it was, so one click brings it back.
    if (sel.layout !== before.layout || sel.theme !== before.theme) {
      invoke("setup_save", { name: LOOKS_PREVIOUS, layout: before.layout, theme: before.theme }).catch(() => {});
    }
    ui.looks.active_layout = sel.layout;
    ui.looks.active_theme = sel.theme;
    invoke("set_active_look", { layout: sel.layout, theme: sel.theme }).catch(showError);
  } else {
    ui.looks.active_layout = before.layout;
    ui.looks.active_theme = before.theme;
    applyActiveLooks();
  }
}

// "#abc" / "#aabbccdd" → "#aabbcc" for <input type=color>.
function toHex6(c) {
  if (typeof c !== "string" || !c.startsWith("#")) return "#000000";
  const h = c.slice(1);
  if (h.length === 3 || h.length === 4) return `#${[...h.slice(0, 3)].map((x) => x + x).join("")}`;
  return `#${h.slice(0, 6)}`;
}

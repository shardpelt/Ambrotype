// SPDX-License-Identifier: GPL-3.0-or-later
// The Appearance dialog: layouts, themes and the toolbar's order. Split out of
// layout.js.
"use strict";

// ---------------------------------------------------------------- Appearance dialog

async function looksFlow() {
  const dlg = $("looks-dialog");
  if (!ui.looks) return toast("Layouts and themes aren't available", true);
  const before = { layout: ui.looks.active_layout, theme: ui.looks.active_theme };
  const sel = { ...before };
  let view = { kind: "layout", id: sel.layout }; // what the name/clone/share/delete row acts on
  let armedDelete = null;

  const viewed = () => findLook(view.kind, view.id);
  const apply = () => {
    const l = findLook("layout", sel.layout), t = findLook("theme", sel.theme);
    applyTheme(t?.usable ? t.data : null);
    if (l?.usable) applyLayout(l.data);
  };
  const choose = (kind, look) => {
    view = { kind, id: look.id };
    if (look.usable) {
      sel[kind] = look.id;
      // A layout may name the theme it was made with.
      const t = kind === "layout" && look.data.theme && findLook("theme", look.data.theme);
      if (t?.usable) sel.theme = t.id;
      apply();
    }
    render();
  };
  const saved = (r) => {
    ui.looks = r.looks;
    const look = findLook(r.kind, r.id);
    if (look) choose(r.kind, look);
  };

  // Saved setups: a layout and theme together, kept as they were.
  let setups = [];
  const PREVIOUS = "Before the last change";
  const loadSetups = async () => {
    setups = await invoke("setups_list").catch(() => []);
    renderSetups();
  };
  function renderSetups() {
    const list = $("looks-setups-list");
    if (!setups.length) {
      list.replaceChildren(Object.assign(document.createElement("p"), { className: "hint", textContent: "None yet." }));
      return;
    }
    list.replaceChildren(...setups.map((s) => {
      const row = document.createElement("div");
      row.className = "looks-setup";
      const when = new Date(s.saved * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
      const name = Object.assign(document.createElement("span"), {
        textContent: s.name,
        title: `${s.layout.data?.name ?? s.layout.id} + ${s.theme.data?.name ?? s.theme.id}, kept ${when}`,
      });
      const what = Object.assign(document.createElement("span"), {
        className: "hint", textContent: `${s.layout.data?.name ?? "?"} · ${s.theme.data?.name ?? "?"}`,
      });
      const use = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "Use" });
      use.addEventListener("click", async () => {
        try {
          const r = await invoke("setup_restore", { name: s.name });
          ui.looks = r.looks;
          sel.layout = r.layout;
          sel.theme = r.theme;
          view = { kind: "layout", id: sel.layout };
          apply();
          render();
          toast(`"${s.name}" is back: Done keeps it`);
        } catch (e) {
          showError(e);
        }
      });
      const del = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "×", title: "Forget this setup (its layout and theme stay)" });
      del.addEventListener("click", async () => {
        setups = await invoke("setup_delete", { name: s.name }).catch((e) => { showError(e); return setups; });
        renderSetups();
      });
      row.append(name, what, use, del);
      return row;
    }));
  }
  const keepSetup = async () => {
    const l = findLook("layout", sel.layout), t = findLook("theme", sel.theme);
    const name = await askName("Save this setup", "The chosen layout and theme, exactly as they are now.", `${l?.name ?? "Layout"} + ${t?.name ?? "theme"}`);
    if (!name) return;
    try {
      setups = await invoke("setup_save", { name, layout: sel.layout, theme: sel.theme });
      renderSetups();
      toast(`Saved the setup "${name}"`);
    } catch (e) {
      showError(e);
    }
  };
  $("looks-keep").onclick = keepSetup;
  loadSetups();

  function render() {
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
        const marks = document.createElement("span");
        marks.className = "marks";
        marks.textContent = (look.problems?.length ? "⚠ " : "") + (look.builtin ? "🔒" : "");
        b.title = look.builtin ? "Built in: clone it to make changes" : (look.usable ? "" : "Can't be used: see the problems below");
        b.append(name, marks);
        b.addEventListener("click", () => choose(kind, look));
        return b;
      }));
    }
    const look = viewed();
    $("looks-name").value = look?.name ?? "";
    $("looks-name").disabled = !look || look.builtin || !look.usable;
    $("looks-clone").disabled = !look?.usable;
    $("looks-share").disabled = !look?.usable;
    $("looks-delete").disabled = !look || look.builtin;
    $("looks-submit").disabled = !look || look.builtin || !look.usable;
    $("looks-delete").textContent = armedDelete === look?.id ? "Really delete?" : "Delete";
    const about = [];
    if (look?.data?.description) about.push(look.data.description);
    const base = look?.based_on && findLook(view.kind, look.based_on);
    if (look?.based_on) about.push(`Based on ${base?.name ?? look.based_on}.`);
    if (look && !look.builtin) about.push(`Your ${view.kind}: a file in your own folder (Open folder).`);
    $("looks-about").textContent = about.join(" ");
    $("looks-problems").replaceChildren(...(look?.problems ?? []).map((p) => {
      const li = document.createElement("li");
      li.className = p.error ? "error" : "warning";
      li.textContent = `${p.error ? "Error" : "Warning"}: ${p.message}`;
      return li;
    }));
    renderThemeEditor(view.kind === "theme" && look && !look.builtin && look.usable ? look : null);
  }

  // Colour pickers for one of the user's themes; changes show at once and save on release.
  function renderThemeEditor(look) {
    const box = $("theme-editor");
    box.hidden = !look;
    if (!look) return box.replaceChildren();
    const data = structuredClone(look.data);
    data.tokens ??= {};
    const fallback = findLook("theme", "builtin:dark")?.data.tokens ?? {};
    const save = async () => {
      try {
        saved(await invoke("save_look", { kind: "theme", id: look.id, data }));
      } catch (e) { showError(e); }
    };
    const rows = Object.keys(fallback).map((token) => {
      const label = document.createElement("label");
      label.className = "token";
      const value = data.tokens[token] ?? fallback[token];
      let input;
      if (typeof fallback[token] === "number") {
        input = Object.assign(document.createElement("input"), { type: "range", min: 0, max: 16, step: 1, value });
      } else {
        input = Object.assign(document.createElement("input"), { type: "color", value: toHex6(value) });
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

  const handlers = {
    "looks-clone": async () => {
      const look = viewed();
      const data = structuredClone(look.data);
      const names = new Set(ui.looks[`${view.kind}s`].map((l) => l.name));
      let name = `${look.name} copy`;
      for (let n = 2; names.has(name); n++) name = `${look.name} copy ${n}`;
      data.name = name;
      data.based_on = look.id;
      saved(await invoke("save_look", { kind: view.kind, id: null, data }));
      $("looks-name").focus();
      $("looks-name").select();
    },
    "looks-share": async () => {
      const look = viewed();
      const path = await invoke("export_look", { kind: view.kind, id: look.id, name: look.name });
      if (path) toast(`Saved ${path}`);
    },
    "looks-delete": async () => {
      const look = viewed();
      if (armedDelete !== look.id) {
        armedDelete = look.id;
        setTimeout(() => { if (armedDelete === look.id) { armedDelete = null; render(); } }, 3000);
        return render();
      }
      armedDelete = null;
      ui.looks = await invoke("delete_look", { kind: view.kind, id: look.id });
      const fallback = view.kind === "layout" ? "builtin:classic" : "builtin:dark";
      if (sel[view.kind] === look.id) sel[view.kind] = fallback;
      if (before[view.kind] === look.id) before[view.kind] = fallback;
      view = { kind: view.kind, id: sel[view.kind] };
      apply();
      render();
    },
    "looks-import": async () => {
      const r = await invoke("import_look");
      if (r) { saved(r); toast(`Imported "${findLook(r.kind, r.id)?.name}"`); }
    },
    "looks-folder": async () => toast(`Your looks: ${await invoke("open_looks_folder")}`),
    "looks-hub": async () => {
      await hubFlow();
      // Something may have been installed (or switched to with "Use now").
      ui.looks = await invoke("looks");
      sel.layout = ui.looks.active_layout;
      sel.theme = ui.looks.active_theme;
      view = { kind: view.kind, id: sel[view.kind] };
      render();
    },
    "looks-submit": async () => {
      const look = viewed();
      await invoke("hub_submit", { kind: view.kind, id: look.id });
      toast(`GitHub is open with your ${view.kind} filled in: press Propose changes, then Create pull request.`);
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
      if (!viewed()) view = { kind: "layout", id: sel.layout };
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
      saved(await invoke("save_look", { kind: view.kind, id: look.id, data: { ...look.data, name } }));
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
      invoke("setup_save", { name: PREVIOUS, layout: before.layout, theme: before.theme }).catch(() => {});
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

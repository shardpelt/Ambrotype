// SPDX-License-Identifier: GPL-3.0-or-later
// The Plugins dialog: run plugins on the open photo, allow their permissions,
// install and remove them, share one as a pull request, and write
// instructions for the user's own AI to make a new one. Plugins live in
// src/plugins.rs and run sandboxed in crates/amb-plugin; their edits come
// back as ordinary commands (one undo step).
"use strict";

const pluginUi = {
  list: [],               // PluginEntry from src/plugins.rs
  selected: null,         // plugin id
  params: new Map(),      // id -> { name: value } chosen this session
  recipes: {},            // id -> the user's saved recipes [{ name, params }]
  recipe: new Map(),      // id -> the recipe picked ("b:<name>" built in, "u:<name>" yours)
  batch: null,            // photo paths to run on, when opened from the library
  wired: false,
};

const pluginById = (id) => pluginUi.list.find((p) => p.id === id);

async function loadPlugins() {
  pluginUi.list = await invoke("plugins_list");
  pluginUi.recipes = await invoke("plugin_recipes").catch(() => ({}));
  if (!pluginById(pluginUi.selected)) pluginUi.selected = pluginUi.list[0]?.id ?? null;
}

function renderPluginList() {
  const list = $("plugins-list");
  list.replaceChildren();
  for (const p of pluginUi.list) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "plugin-item";
    b.setAttribute("role", "option");
    b.classList.toggle("on", p.id === pluginUi.selected);
    b.classList.toggle("broken", Boolean(p.error));
    const name = Object.assign(document.createElement("span"), { textContent: p.manifest?.name ?? p.id });
    const tag = Object.assign(document.createElement("span"), {
      className: "plugin-tag",
      textContent: p.error ? "can't load" : p.builtin ? "built-in" : "yours",
    });
    b.append(name, tag);
    b.addEventListener("click", () => {
      pluginUi.selected = p.id;
      showPluginPanel("detail");
      renderPlugins();
    });
    list.append(b);
  }
  if (!pluginUi.list.length) list.append(Object.assign(document.createElement("p"), { className: "hint", textContent: "No plugins." }));
}

function paramValues(p) {
  if (!pluginUi.params.has(p.id)) pluginUi.params.set(p.id, {});
  const chosen = pluginUi.params.get(p.id);
  const out = {};
  for (const def of p.manifest?.params ?? []) out[def.name] = chosen[def.name] ?? def.default;
  return out;
}

// Controls generated from the manifest's parameters.
function renderPluginParams(p) {
  const box = $("plugin-params");
  box.replaceChildren();
  const values = paramValues(p);
  const chosen = pluginUi.params.get(p.id);
  for (const def of p.manifest?.params ?? []) {
    const row = document.createElement("label");
    row.className = "plugin-param";
    row.title = def.description || "";
    const label = Object.assign(document.createElement("span"), { textContent: def.label });
    row.append(label);
    if (def.type === "number") {
      const range = Object.assign(document.createElement("input"), { type: "range", min: def.min, max: def.max, step: def.step ?? "any", value: values[def.name] });
      const out = Object.assign(document.createElement("output"), { textContent: values[def.name] });
      range.addEventListener("input", () => {
        chosen[def.name] = Number(range.value);
        out.textContent = range.value;
        detachRecipe(p);
      });
      range.addEventListener("dblclick", () => {
        delete chosen[def.name];
        range.value = def.default;
        out.textContent = def.default;
      });
      const pair = Object.assign(document.createElement("span"), { className: "plugin-range" });
      pair.append(range, out);
      row.append(pair);
    } else if (def.type === "choice") {
      const sel = document.createElement("select");
      for (const o of def.options) sel.append(Object.assign(document.createElement("option"), { value: o, textContent: o }));
      sel.value = values[def.name];
      sel.addEventListener("change", () => {
        chosen[def.name] = sel.value;
        detachRecipe(p);
      });
      row.append(sel);
    } else {
      row.classList.add("check");
      const box2 = Object.assign(document.createElement("input"), { type: "checkbox", checked: Boolean(values[def.name]) });
      box2.addEventListener("change", () => {
        chosen[def.name] = box2.checked;
        detachRecipe(p);
      });
      row.prepend(box2);
    }
    box.append(row);
  }
}

// Settings changed by hand are no longer the picked recipe (a ready-made one).
function detachRecipe(p) {
  if ((pluginUi.recipe.get(p.id) ?? "").startsWith("b:")) {
    pluginUi.recipe.set(p.id, "");
    $("plugin-recipe").value = "";
  }
}

function renderPluginDetail() {
  const p = pluginById(pluginUi.selected);
  $("plugin-detail").hidden = !p || !$("plugin-ai").hidden;
  if (!p) return;
  const m = p.manifest;
  $("plugin-name").textContent = m?.name ?? p.id;
  $("plugin-meta").textContent = m ? [m.version && `v${m.version}`, m.author && `by ${m.author}`, p.builtin ? "built in" : "yours"].filter(Boolean).join(" · ") : "";
  $("plugin-desc").textContent = m?.description ?? "";
  $("plugin-perms").textContent = !m ? "" : p.permissions.length
    ? `Asks to: ${p.permissions.join("; ")}.${p.approved ? " (Allowed.)" : ""}`
    : "Asks for no permissions: it sees only the photo's settings.";
  $("plugin-error").hidden = !p.error;
  $("plugin-error").textContent = p.error ?? "";
  renderRecipes(p);
  renderPluginParams(p);
  $("plugin-run").disabled = Boolean(p.error) || !ui.info;
  $("plugin-as-preset").disabled = !ui.info;
  $("plugin-run").title = ui.info ? "Run it on the open photo (Undo takes it back)" : "Open a photo first";
  $("plugin-defaults").hidden = !(m?.params?.length);
  $("plugin-own").hidden = p.builtin;
  $("plugin-share").disabled = Boolean(p.error);
  $("plugin-approve").hidden = true;
}

// Recipes: ready-made (from the manifest) and yours (saved in the settings).
function renderRecipes(p) {
  const sel = $("plugin-recipe");
  const builtIn = p.manifest?.recipes ?? [];
  const yours = pluginUi.recipes[p.id] ?? [];
  sel.replaceChildren(Object.assign(document.createElement("option"), { value: "", textContent: "— the settings below —" }));
  for (const [label, list, tag] of [["Ready-made", builtIn, "b"], ["Yours", yours, "u"]]) {
    if (!list.length) continue;
    const og = Object.assign(document.createElement("optgroup"), { label });
    for (const r of list) og.append(Object.assign(document.createElement("option"), { value: `${tag}:${r.name}`, textContent: r.name }));
    sel.append(og);
  }
  sel.value = pluginUi.recipe.get(p.id) ?? "";
  const picked = sel.value;
  $("plugin-recipe-delete").disabled = !picked.startsWith("u:");
  if (picked.startsWith("u:") && document.activeElement !== $("plugin-recipe-name")) $("plugin-recipe-name").value = picked.slice(2);
  $("plugin-recipes").hidden = !p.manifest || !(p.manifest.params ?? []).length;
}

function pickRecipe() {
  const p = pluginById(pluginUi.selected);
  const v = $("plugin-recipe").value;
  pluginUi.recipe.set(p.id, v);
  if (v) {
    const list = v.startsWith("b:") ? p.manifest.recipes : pluginUi.recipes[p.id] ?? [];
    const r = list.find((x) => x.name === v.slice(2));
    if (r) pluginUi.params.set(p.id, { ...r.params });
    $("plugin-recipe-name").value = v.startsWith("u:") ? v.slice(2) : "";
  }
  renderPluginDetail();
}

async function saveRecipe() {
  const p = pluginById(pluginUi.selected);
  const name = $("plugin-recipe-name").value.trim();
  if (!name) {
    $("plugin-recipe-name").focus();
    return toast("Give the recipe a name first.", true);
  }
  try {
    pluginUi.recipes = await invoke("save_plugin_recipe", { id: p.id, name, params: paramValues(p) });
    pluginUi.recipe.set(p.id, `u:${name}`);
    renderPluginDetail();
    toast(`Saved the recipe “${name}”`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function deleteRecipe() {
  const p = pluginById(pluginUi.selected);
  const v = pluginUi.recipe.get(p.id) ?? "";
  if (!v.startsWith("u:")) return;
  try {
    pluginUi.recipes = await invoke("delete_plugin_recipe", { id: p.id, name: v.slice(2) });
    pluginUi.recipe.set(p.id, "");
    $("plugin-recipe-name").value = "";
    renderPluginDetail();
  } catch (e) {
    toast(String(e), true);
  }
}

function renderPlugins() {
  renderPluginList();
  renderPluginDetail();
}

function showPluginPanel(which) {
  $("plugin-share-panel").hidden = which !== "share";
  $("plugin-ai").hidden = which !== "ai";
  $("plugin-detail").hidden = which === "ai";
  if (which !== "result") {
    $("plugin-result").textContent = "";
    $("plugin-log").hidden = true;
  }
}

// Run on each photo selected in the library, with its own settings.
async function runPluginBatch(p) {
  const paths = pluginUi.batch;
  $("plugin-result").textContent = "Running…";
  const off = await tauri.event.listen("library-progress", (e) => {
    const q = e.payload;
    if (q.done < q.total) $("plugin-result").textContent = `Running on ${q.done + 1} of ${q.total}: ${q.name}`;
  });
  try {
    const r = await invoke("run_plugin_many", { id: p.id, params: paramValues(p), paths });
    for (const path of r.changed) {
      const photo = libPhoto(path);
      if (photo) {
        photo.edited = true;
        updateCell(photo);
        stripUpdate(path, { edited: true });
      }
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    const failed = r.errors.length ? ` · ${r.errors.length} failed: ${r.errors[0]}` : "";
    const note = `Changed ${r.changed.length} of ${paths.length} photo${paths.length === 1 ? "" : "s"}${failed}`;
    $("plugin-result").textContent = [r.message, note].filter(Boolean).join(" · ");
    toast(`${p.manifest.name}: ${note}`, r.errors.length > 0);
  } finally {
    off();
  }
}

async function runSelectedPlugin() {
  const p = pluginById(pluginUi.selected);
  if (!p || (!ui.info && !pluginUi.batch)) return;
  $("plugin-run").disabled = true;
  $("plugin-result").textContent = "Running…";
  try {
    if (pluginUi.batch) return await runPluginBatch(p);
    const r = await invoke("run_plugin", { id: p.id, params: paramValues(p) });
    setState(r.state);
    requestPreview(false);
    const note = r.message || (r.edits ? `${r.edits} change(s)` : "No changes");
    $("plugin-result").textContent = [note, ...r.notes].join(" · ");
    toast(`${p.manifest.name}: ${note}`);
    $("plugin-log").hidden = !r.log.length;
    $("plugin-log").querySelector("pre").textContent = r.log.join("\n");
  } catch (e) {
    if (String(e) === "needs-approval") {
      $("plugin-result").textContent = "";
      $("plugin-approve-name").textContent = p.manifest.name;
      $("plugin-approve-list").replaceChildren(...p.permissions.map((t) => Object.assign(document.createElement("li"), { textContent: t })));
      $("plugin-approve").hidden = false;
      $("plugin-approve").scrollIntoView({ block: "nearest" });
    } else {
      $("plugin-result").textContent = String(e);
    }
  } finally {
    $("plugin-run").disabled = false;
  }
}

async function allowSelectedPlugin() {
  const p = pluginById(pluginUi.selected);
  if (!p) return;
  try {
    pluginUi.list = await invoke("approve_plugin", { id: p.id, fingerprint: p.fingerprint });
    renderPlugins();
    await runSelectedPlugin();
  } catch (e) {
    toast(String(e), true);
  }
}

async function installPlugin() {
  try {
    const r = await invoke("install_plugin");
    if (!r) return;
    [pluginUi.list, pluginUi.selected] = r;
    showPluginPanel("detail");
    renderPlugins();
    toast(`Installed ${pluginById(pluginUi.selected)?.manifest?.name ?? pluginUi.selected}`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function removeSelectedPlugin() {
  const p = pluginById(pluginUi.selected);
  if (!p || p.builtin) return;
  if (!confirm(`Remove ${p.manifest?.name ?? p.id}? Its folder is deleted.`)) return;
  try {
    pluginUi.list = await invoke("remove_plugin", { id: p.id });
    pluginUi.selected = pluginUi.list[0]?.id ?? null;
    renderPlugins();
  } catch (e) {
    toast(String(e), true);
  }
}

// Sharing: show exactly what would be sent, then send it only when asked.
async function shareSelectedPlugin() {
  const p = pluginById(pluginUi.selected);
  if (!p) return;
  try {
    const plan = await invoke("plugin_share_plan", { id: p.id });
    showPluginPanel("share");
    $("share-what").textContent = `These files go to ${plan.target} in ${plan.repo}, as a pull request from your GitHub account. The hub's reviewers check it before anyone can install it.`;
    $("share-files").replaceChildren(...plan.files.map((f) => Object.assign(document.createElement("li"), {
      textContent: `${f.path}  (${f.bytes < 1000 ? f.bytes + " B" : Math.round(f.bytes / 1000) + " KB"})`,
    })));
    $("share-warn").hidden = plan.has_source;
    $("share-warn").textContent = "There is no source code in the folder. Reviewers need it to check and rebuild plugin.wasm; add the plugin's src/ and Cargo.toml.";
    $("share-gh").hidden = !plan.gh_user;
    $("share-gh").textContent = plan.gh_user ? `Send pull request as @${plan.gh_user}` : "";
    $("share-how").textContent = plan.gh_user
      ? "The GitHub CLI on this computer sends it with your own login: it forks the hub, adds the files in one commit and opens the pull request."
      : "In the browser: GitHub's upload page opens with the plugin's folder next to it. Drag the files in and choose “Propose changes”. (Or install the GitHub CLI and run `gh auth login` to send it from here.)";
  } catch (e) {
    toast(String(e), true);
  }
}

async function sendShare(browser) {
  const p = pluginById(pluginUi.selected);
  if (!p) return;
  $("share-how").textContent = browser ? "Opening GitHub…" : "Sending…";
  try {
    const url = await invoke("share_plugin", { id: p.id, browser });
    toast(browser ? "GitHub's upload page is open in your browser" : "Pull request opened");
    $("share-how").textContent = browser ? `Opened ${url}` : `Pull request: ${url}`;
  } catch (e) {
    $("share-how").textContent = String(e);
  }
}

// Instructions for the user's own AI assistant (no keys or accounts here).
async function updateAiPrompt() {
  const want = $("ai-request").value.trim() || "<describe what it should do>";
  let folder = "<your Ambrotype settings folder>/plugins";
  try {
    const info = await invoke("settings_info");
    if (info.folder) folder = `${info.folder}/plugins`;
  } catch {}
  const slug = (want.toLowerCase().match(/[a-z0-9]+/g) ?? ["my", "plugin"]).slice(0, 4).join("-");
  $("ai-prompt").value = `Make an Ambrotype plugin that does this: ${want}

Ambrotype is an open-source raw photo editor: https://github.com/shardpelt/Ambrotype
Plugins are small Rust programs built to WebAssembly. First read docs/ai/plugins.md and docs/ai/settings-reference.md in that repo (clone it if you haven't).

1. Create the plugin: \`cargo run -p amb-cli -- plugin new "${folder}/${slug}"\` (from the Ambrotype repo).
2. Write src/lib.rs and plugin.json: a good name, a one-line description, parameters the user can set, and only the permissions it needs. Use only setting names from the settings reference.
3. Run \`cargo test\` in the plugin folder, then \`cargo run -p amb-cli -- plugin build "${folder}/${slug}"\`, and fix anything it reports.
4. Tell me what it does and which parameters it has. I'll press Reload in Ambrotype's Plugins dialog to try it.`;
}

function wirePlugins() {
  if (pluginUi.wired) return;
  pluginUi.wired = true;
  $("plugin-run").addEventListener("click", runSelectedPlugin);
  $("plugin-defaults").addEventListener("click", () => {
    pluginUi.params.delete(pluginUi.selected);
    pluginUi.recipe.set(pluginUi.selected, "");
    renderPluginDetail();
  });
  $("plugin-recipe").addEventListener("change", pickRecipe);
  $("plugin-recipe-save").addEventListener("click", saveRecipe);
  $("plugin-recipe-delete").addEventListener("click", deleteRecipe);
  $("plugin-as-preset").addEventListener("click", () => {
    const v = pluginUi.recipe.get(pluginUi.selected) ?? "";
    savePresetFlow(v ? v.slice(2) : $("plugin-recipe-name").value.trim());
  });
  $("plugin-allow").addEventListener("click", allowSelectedPlugin);
  $("plugin-deny").addEventListener("click", () => ($("plugin-approve").hidden = true));
  $("plugin-folder").addEventListener("click", () => invoke("open_plugins_folder", { id: pluginUi.selected }).catch((e) => toast(String(e), true)));
  $("plugin-remove").addEventListener("click", removeSelectedPlugin);
  $("plugin-share").addEventListener("click", shareSelectedPlugin);
  $("share-gh").addEventListener("click", () => sendShare(false));
  $("share-browser").addEventListener("click", () => sendShare(true));
  $("share-cancel").addEventListener("click", () => showPluginPanel("detail"));
  $("plugins-install").addEventListener("click", installPlugin);
  $("plugins-hub").addEventListener("click", async () => {
    await hubFlow("plugin");
    await loadPlugins();
    renderPlugins();
  });
  $("plugins-open").addEventListener("click", () => invoke("open_plugins_folder", { id: null }).catch((e) => toast(String(e), true)));
  $("plugins-reload").addEventListener("click", async () => {
    await loadPlugins();
    renderPlugins();
  });
  $("plugins-ai").addEventListener("click", () => {
    showPluginPanel("ai");
    updateAiPrompt();
    $("ai-request").focus();
  });
  $("ai-request").addEventListener("input", updateAiPrompt);
  $("ai-back").addEventListener("click", () => {
    showPluginPanel("detail");
    renderPluginDetail();
  });
  $("ai-copy").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText($("ai-prompt").value);
      toast("Copied: paste it into your AI assistant");
    } catch {
      $("ai-prompt").select();
      toast("Select all and copy (Ctrl+C)", true);
    }
  });
}

async function pluginsFlow() {
  wirePlugins();
  // From the library, a plugin runs on the selected photos.
  const sel = libraryOn() ? selectedPhotos().map((p) => p.path) : [];
  pluginUi.batch = sel.length ? sel : null;
  $("plugin-run").textContent = pluginUi.batch ? `Run on ${sel.length} photo${sel.length === 1 ? "" : "s"}` : "Run";
  await loadPlugins();
  showPluginPanel("detail");
  renderPlugins();
  $("plugins-dialog").showModal();
}

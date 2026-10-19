// SPDX-License-Identifier: GPL-3.0-or-later
// Settings → AI models: the models behind the Subject, Background and People
// masks (src/commands/models.rs, amb-ai). Install one, pick one per task, add
// your own ONNX model, or remove one.
"use strict";

const models = { progress: {}, wired: false };

function megabytes(bytes) {
  return bytes ? `${Math.max(1, Math.round(bytes / 1e6))} MB` : "";
}

// Seconds per photo (on a 4-core laptop) in words.
function modelSpeed(seconds) {
  if (!seconds) return "";
  if (seconds < 1) return "under a second a photo";
  if (seconds < 60) return `about ${Math.round(seconds)} s a photo`;
  return `about ${Math.round(seconds / 60)} min a photo`;
}

function modelsMessage(text) {
  $("models-message").textContent = text;
}

// After a model change the photo's masks render again.
function modelsChanged(list) {
  renderModels(list);
  if (ui.info) requestPreview(false);
}

async function modelAction(cmd, id, done) {
  try {
    modelsChanged(await invoke(cmd, { id }));
    if (done) modelsMessage(done);
  } catch (e) {
    modelsMessage(String(e));
  }
}

function renderModels(list) {
  const box = $("models-list");
  box.replaceChildren();
  for (const [task, about] of list.tasks) {
    box.append(el("h4", {}, `${task[0].toUpperCase()}${task.slice(1)}: ${about}`));
    for (const m of list.models.filter((x) => x.task === task)) {
      const busy = models.progress[m.id];
      const actions = [];
      if (m.chosen && m.installed) actions.push(el("span", { class: "model-in-use" }, "In use"));
      else if (m.installed) actions.push(el("button", { type: "button", class: "small", onclick: () => modelAction("ai_model_use", m.id, `${m.name} is in use for ${task} masks.`) }, "Use"));
      if (!m.installed && busy == null) {
        actions.push(el("button", { type: "button", class: "small", title: m.urls?.[0] ?? "", onclick: () => installModel(m) }, `Install (${megabytes(m.bytes)})`));
      }
      if (busy != null) actions.push(el("span", { class: "hint" }, `Downloading… ${busy}%`));
      if (m.installed && (m.custom || !m.default)) {
        actions.push(el("button", { type: "button", class: "small", onclick: () => modelAction("ai_model_remove", m.id, `Removed ${m.name}.`) }, "Remove"));
      }
      const facts = [megabytes(m.bytes), modelSpeed(m.seconds), m.licence].filter(Boolean).join(" · ");
      box.append(el("div", { class: "model-row" + (m.chosen ? " chosen" : "") },
        el("div", { class: "model-text" },
          el("strong", {}, m.name), " ",
          m.detail ? el("span", { class: "model-detail", title: `Edge detail ${m.detail} of 5` }, "●".repeat(m.detail) + "○".repeat(5 - m.detail)) : "",
          " ", el("span", { class: "hint" }, facts),
          el("div", { class: "hint" }, m.about + (m.notes ? ` Note: ${m.notes}` : "") + (m.chosen && !m.installed ? " It downloads by itself the first time a mask needs it." : "")),
          m.source ? el("div", { class: "hint model-source" }, m.source) : ""),
        el("div", { class: "model-actions" }, ...actions)));
    }
  }
}

async function installModel(m) {
  models.progress[m.id] = 0;
  modelsMessage(`Downloading ${m.name} (${megabytes(m.bytes)})…`);
  renderModels(await invoke("ai_models"));
  try {
    const list = await invoke("ai_model_install", { id: m.id });
    delete models.progress[m.id];
    renderModels(list);
    modelsMessage(`${m.name} is installed. Click Use to use it for ${m.task} masks.`);
  } catch (e) {
    delete models.progress[m.id];
    renderModels(await invoke("ai_models"));
    modelsMessage(String(e));
  }
}

function triple(id) {
  const v = $(id).value.split(",").map((x) => Number(x.trim()));
  if (v.length !== 3 || v.some((x) => !Number.isFinite(x))) throw new Error(`${id.replace("model-", "")}: three numbers, like 0.5,0.5,0.5`);
  return v;
}

async function addOwnModel() {
  try {
    const name = $("model-name").value.trim();
    if (!name) return modelsMessage("Give the model a name first.");
    const model = {
      task: $("model-task").value,
      name,
      size: Number($("model-size").value),
      mean: triple("model-mean"),
      std: triple("model-std"),
      activation: $("model-sigmoid").checked ? "sigmoid" : "none",
      licence: $("model-licence").value.trim(),
    };
    const file = await invoke("pick_model_file");
    if (!file) return;
    modelsMessage("Checking that it runs…");
    renderModels(await invoke("ai_model_add", { file, model }));
    modelsMessage(`Added ${name}. Click Use to use it.`);
  } catch (e) {
    modelsMessage(String(e));
  }
}

async function modelsTab() {
  const list = await invoke("ai_models");
  if (!models.wired) {
    models.wired = true;
    $("model-task").replaceChildren(...list.tasks.map(([t, about]) => el("option", { value: t }, `${t} (${about})`)));
    $("model-add").addEventListener("click", addOwnModel);
    tauri.event.listen("model-progress", (e) => {
      const { id, percent } = e.payload;
      if (models.progress[id] == null) return;
      models.progress[id] = percent;
      const row = [...document.querySelectorAll("#models-list .model-row .hint")].find((h) => h.textContent.startsWith("Downloading…") && h.closest(".model-row")?.querySelector("strong")?.textContent === list.models.find((m) => m.id === id)?.name);
      if (row) row.textContent = `Downloading… ${percent}%`;
    });
  }
  modelsMessage("");
  renderModels(list);
}

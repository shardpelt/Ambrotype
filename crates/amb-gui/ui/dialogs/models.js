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
  if (seconds < 1) return t("under a second a photo");
  if (seconds < 60) return t("about {n} s a photo", { n: Math.round(seconds) });
  return t("about {n} min a photo", { n: Math.round(seconds / 60) });
}

function modelsMessage(text) {
  $("models-message").textContent = text;
}

// After a model change the photo's masks render again.
function modelsChanged(list) {
  renderModels(list);
  if (ui.info) requestPreview(false);
}

// What picking an installed model for `task` says.
function modelInUse(task, name) {
  if (task === "search") return t("Smart search now uses {name}.", { name });
  if (task === "remove") return t("AI Remove now uses {name}.", { name });
  if (task === "objects") return t("Remove object now selects with {name}.", { name });
  return t("{name} is in use for {task} masks.", { name, task });
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
  // Where they run: our GPU kernels, or the CPU (and why).
  $("models-runs-on").textContent = list.runs_on ? t("Models run {where}. Small ones stay on the CPU, where they are quicker.", { where: list.runs_on }) : "";
  const box = $("models-list");
  box.replaceChildren();
  for (const [task, about] of list.tasks) {
    box.append(el("h4", {}, t("{task}: {about}", { task: `${task[0].toUpperCase()}${task.slice(1)}`, about: t(about) })));
    for (const m of list.models.filter((x) => x.task === task)) {
      const busy = models.progress[m.id];
      const actions = [];
      if (m.chosen && m.installed) actions.push(el("span", { class: "model-in-use" }, t("In use")));
      else if (m.installed) actions.push(el("button", { type: "button", class: "small", onclick: () => modelAction("ai_model_use", m.id, modelInUse(task, m.name)) }, t("Use")));
      if (!m.installed && busy == null) {
        actions.push(el("button", { type: "button", class: "small", title: m.urls?.[0] ?? "", onclick: () => installModel(m) }, t("Install ({size})", { size: megabytes(m.total_bytes ?? m.bytes) })));
      }
      if (busy != null) actions.push(el("span", { class: "hint model-progress", "data-model": m.id }, t("Downloading… {percent}%", { percent: busy })));
      if (m.installed && (m.custom || !m.default)) {
        actions.push(el("button", { type: "button", class: "small", onclick: () => modelAction("ai_model_remove", m.id, t("Removed {name}.", { name: m.name })) }, t("Remove")));
      }
      const facts = [megabytes(m.total_bytes ?? m.bytes), modelSpeed(m.seconds), m.licence].filter(Boolean).join(" · ");
      box.append(el("div", { class: "model-row" + (m.chosen ? " chosen" : "") },
        el("div", { class: "model-text" },
          el("strong", { translate: "no" }, m.name), " ",
          m.detail ? el("span", { class: "model-detail", title: t("Edge detail {n} of 5", { n: m.detail }) }, "●".repeat(m.detail) + "○".repeat(5 - m.detail)) : "",
          " ", el("span", { class: "hint" }, facts),
          el("div", { class: "hint" }, [
            t(m.about),
            m.notes && t("Note: {notes}", { notes: m.notes }),
            m.chosen && !m.installed && t("It downloads by itself the first time a mask needs it."),
          ].filter(Boolean).join(" ")),
          m.source ? el("div", { class: "hint model-source", translate: "no" }, m.source) : ""),
        el("div", { class: "model-actions" }, ...actions)));
    }
  }
}

async function installModel(m) {
  models.progress[m.id] = 0;
  modelsMessage(t("Downloading {name} ({size})…", { name: m.name, size: megabytes(m.total_bytes ?? m.bytes) }));
  renderModels(await invoke("ai_models"));
  try {
    const list = await invoke("ai_model_install", { id: m.id });
    delete models.progress[m.id];
    renderModels(list);
    modelsMessage(t("{name} is installed. Click Use to use it for {task} masks.", { name: m.name, task: m.task }));
  } catch (e) {
    delete models.progress[m.id];
    renderModels(await invoke("ai_models"));
    modelsMessage(String(e));
  }
}

function triple(id) {
  const v = $(id).value.split(",").map((x) => Number(x.trim()));
  if (v.length !== 3 || v.some((x) => !Number.isFinite(x))) throw new Error(t("{field}: three numbers, like 0.5,0.5,0.5", { field: id.replace("model-", "") }));
  return v;
}

async function addOwnModel() {
  try {
    const name = $("model-name").value.trim();
    if (!name) return modelsMessage(t("Give the model a name first."));
    const model = {
      task: $("model-task").value,
      name,
      size: Number($("model-size").value),
      mean: triple("model-mean"),
      std: triple("model-std"),
      activation: $("model-activation").value,
      layout: $("model-layout").value,
      channel: $("model-channel").value === "" ? null : Number($("model-channel").value),
      invert: $("model-invert").checked,
      licence: $("model-licence").value.trim(),
    };
    const file = await invoke("pick_model_file");
    if (!file) return;
    modelsMessage(t("Checking that it runs…"));
    renderModels(await invoke("ai_model_add", { file, model }));
    modelsMessage(t("Added {name}. Click Use to use it.", { name }));
  } catch (e) {
    modelsMessage(String(e));
  }
}

async function modelsTab() {
  const list = await invoke("ai_models");
  if (!models.wired) {
    models.wired = true;
    $("model-task").replaceChildren(...list.tasks.map(([task, about]) => el("option", { value: task }, t("{task} ({about})", { task, about: t(about) }))));
    $("model-add").addEventListener("click", addOwnModel);
    tauri.event.listen("model-progress", (e) => {
      const { id, percent } = e.payload;
      if (models.progress[id] == null) return;
      models.progress[id] = percent;
      const row = [...document.querySelectorAll("#models-list .model-progress")].find((h) => h.dataset.model === id);
      if (row) row.textContent = t("Downloading… {percent}%", { percent });
    });
  }
  modelsMessage("");
  renderModels(list);
}

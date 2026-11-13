// SPDX-License-Identifier: GPL-3.0-or-later
// AI Denoise (the library's More, or search): the selected raws (or the open
// photo), each into a new DNG beside it without its noise, with its edit
// (amb_ops::denoise). The model is an optional download; the dialog offers
// it when it's missing. Rust side: photo_denoise in commands/batch.rs.
"use strict";

const denoise = { photos: [], model: null };

function denoiseTargets() {
  if (libraryOn()) return selectedPhotos().filter((p) => !p.copy).map((p) => p.path);
  return ui.info?.path ? [ui.info.path] : [];
}

async function denoiseModel() {
  const list = await invoke("ai_models");
  return list.models.find((m) => m.task === "denoise" && m.chosen) ?? list.models.find((m) => m.task === "denoise");
}

function renderDenoise() {
  const m = denoise.model;
  const ready = Boolean(m?.installed);
  $("denoise-install").hidden = ready;
  $("denoise-install").textContent = m ? `Install ${m.name} (${Math.round(m.total_bytes / 1e6)} MB)` : "No model";
  $("denoise-go").disabled = !ready || !denoise.photos.length;
  $("denoise-model").textContent = m
    ? `${m.name} (${m.licence}), on your computer. ${ready ? "" : "Download it once to start."}`
    : "";
}

async function denoiseFlow() {
  denoise.photos = denoiseTargets();
  if (!denoise.photos.length) return toast("Select the raws to denoise first (or open one).", true);
  const n = denoise.photos.length;
  $("denoise-what").textContent = `${plural(n, "photo")}: ${denoise.photos.map(fileName).slice(0, 4).join(", ")}${n > 4 ? "…" : ""}. Each becomes a new DNG beside it (name-Denoised.dng), with its edit; the raw stays as it is.`;
  $("denoise-status").textContent = "";
  denoise.model = await denoiseModel().catch(() => null);
  renderDenoise();
  $("denoise-dialog").showModal();
}

async function installDenoiseModel() {
  const m = denoise.model;
  if (!m) return;
  $("denoise-status").textContent = "Downloading…";
  await withBusy($("denoise-install"), $("denoise-status"), async () => {
    await invoke("ai_model_install", { id: m.id });
    denoise.model = await denoiseModel();
    $("denoise-status").textContent = "";
  });
  renderDenoise();
}

async function runDenoise() {
  $("denoise-status").textContent = "Starting…";
  await withBusy($("denoise-go"), $("denoise-status"), async () => {
    const reports = await invoke("photo_denoise", { paths: denoise.photos, amount: Number($("denoise-amount").value) });
    $("denoise-dialog").close();
    const first = reports[0]?.output;
    status(`Denoised ${plural(reports.length, "photo")}${first ? `: ${fileName(first)}` : ""} (${Math.round(reports.reduce((s, r) => s + r.seconds, 0))} s)`);
    if (libraryOn()) {
      if (lib.collection) await addToCollection(reports.map((r) => r.output), lib.collection);
      else if (lib.dir) await loadLibrary(lib.dir);
    }
    if (reports.length === 1) await libraryOpen(first);
  });
}

function wireDenoise() {
  $("lib-denoise").addEventListener("click", () => denoiseFlow().catch(showError));
  $("denoise-install").addEventListener("click", installDenoiseModel);
  $("denoise-go").addEventListener("click", runDenoise);
  $("denoise-close").addEventListener("click", () => $("denoise-dialog").close());
  $("denoise-amount").addEventListener("input", () => { $("denoise-amount-value").textContent = $("denoise-amount").value; });
  tauri.event.listen("model-progress", (e) => {
    if ($("denoise-dialog").open && e.payload.id === denoise.model?.id) $("denoise-status").textContent = `Downloading… ${e.payload.percent} %`;
  });
  tauri.event.listen("denoise-progress", (e) => {
    const [i, n, done, total] = e.payload;
    if ($("denoise-dialog").open) $("denoise-status").textContent = `${n > 1 ? `Photo ${i} of ${n}: ` : ""}${Math.round((100 * done) / Math.max(1, total))} %`;
  });
}

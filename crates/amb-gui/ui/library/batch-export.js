// SPDX-License-Identifier: GPL-3.0-or-later
// Batch export (library → Use, File menu): a folder and its subfolders as
// smaller copies in another folder, with the same structure. The work is
// amb_ops::export_batch (src/commands/batch.rs `batch_export`): each photo
// with its own edit, a few at a time, stoppable, and a second run skips
// what is done. Its size controls are the short list the export dialog
// starts with; the export dialog's own form is one closed block of markup
// that reads the open photo, so it can't be reused for a folder.
"use strict";

const batch = { from: "", to: "", dirs: [], running: false, plan: 0, planned: 0 };

const batchNumber = (n) => n.toLocaleString("en-US");
const batchPlural = (n, one) => plural(n, one).replace(String(n), batchNumber(n));
const batchBytes = (b) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${(b / 1e6).toFixed(1)} MB`);

function batchShow(part) {
  $("batch-setup").hidden = part !== "setup";
  $("batch-run").hidden = part !== "run";
  $("batch-summary").hidden = part !== "done";
  $("batch-start").hidden = part === "run";
  $("batch-start").textContent = part === "done" ? t("Again") : t("Start");
  $("batch-stop").hidden = part !== "run";
  $("batch-close").disabled = part === "run";
  if (part !== "done") $("batch-open").hidden = true;
}

// The count line, from what the export would write.
async function batchCount() {
  const n = ++batch.plan;
  $("batch-start").disabled = true;
  try {
    const p = await invoke("batch_export_plan", {
      from: batch.from, to: batch.to || null, recursive: $("batch-sub").checked, keepStructure: $("batch-keep").checked,
    });
    if (n !== batch.plan) return;
    batch.dirs = p.dirs;
    $("batch-count").textContent = p.photos
      ? t("{photos} in {folders}", { photos: batchPlural(p.photos, "photo"), folders: batchPlural(p.folders, "folder") })
      : t("No photos in this folder");
    $("batch-start").disabled = !p.photos || !batch.to;
    batch.planned = n;
  } catch (e) {
    if (n !== batch.plan) return;
    $("batch-count").textContent = String(e);
    batch.planned = n;
  }
}

function batchTo(path) {
  batch.to = path ?? "";
  $("batch-to").textContent = batch.to || t("Not chosen yet");
  batchCount();
}

function openBatchExport() {
  const from = lib.on ? lib.dir : ui.info ? dirOf(ui.info.path) : null;
  if (!from) return toast(t("Open a folder first."), true);
  batch.from = from;
  $("batch-from").textContent = from;
  batchShow("setup");
  $("batch-sub").checked = true;
  batchCount();
  batchTo(store.text("amb.batchTo"));
  $("batch-dialog").showModal();
}

function batchOptions() {
  const value = Number($("batch-value").value);
  const mode = $("batch-mode").value;
  const mb = Number($("batch-mb").value);
  const format = $("batch-format").value;
  return {
    from: batch.from,
    to: batch.to,
    recursive: $("batch-sub").checked,
    keep_structure: $("batch-keep").checked,
    skip_existing: $("batch-skip").checked,
    export: {
      format,
      quality: Number($("batch-quality").value) || 90,
      bits: null,
      color_space: "srgb",
      resize: mode && value > 0 ? { mode, value } : null,
      exact: null,
      sharpen: null,
      metadata: "no-location",
      max_kb: format === "jpeg" && mb > 0 ? Math.round(mb * 1000) : null,
    },
  };
}

async function batchStart() {
  if (batch.running || !batch.to) return;
  const options = batchOptions();
  batch.running = true;
  batchShow("run");
  $("batch-bar").value = 0;
  $("batch-now").textContent = t("Starting…");
  try {
    const r = await invoke("batch_export", { options });
    batchDone(r);
  } catch (e) {
    batchShow("setup");
    $("batch-count").textContent = String(e);
  } finally {
    batch.running = false;
    $("batch-close").disabled = false;
  }
}

function batchDone(r) {
  const gb = `${batchBytes(r.bytes_in)} → ${batchBytes(r.bytes_out)}`;
  const parts = [t("{photos} exported, {sizes}", { photos: batchPlural(r.done, "photo"), sizes: gb })];
  if (r.skipped) parts.push(t("{n} skipped (already there)", { n: batchNumber(r.skipped) }));
  if (r.failed.length) parts.push(t("{n} failed", { n: r.failed.length }));
  if (r.stopped) parts.push(t("stopped: Start again to carry on"));
  $("batch-summary").replaceChildren(
    el("p", {}, t("{parts}, in {seconds} s.", { parts: parts.join(", "), seconds: Math.round(r.seconds) })),
    ...r.failed.slice(0, 50).map(([path, why]) => el("p", { class: "fail" }, `${fileName(path)}: ${why}`)),
    r.failed.length > 50 ? el("p", { class: "hint" }, t("…and {n} more", { n: r.failed.length - 50 })) : null,
  );
  batchShow("done");
  $("batch-open").hidden = false;
  store.setText("amb.batchTo", batch.to);
  toast(t("Batch export: {summary}", { summary: parts[0] }), r.failed.length > 0);
}

function wireBatchExport() {
  $("lib-batch").addEventListener("click", openBatchExport);
  $("batch-pick").addEventListener("click", async () => {
    const dir = await invoke("batch_export_pick", { dir: batch.to || lastExportDir() });
    if (dir) batchTo(dir);
  });
  for (const id of ["batch-sub", "batch-keep"]) $(id).addEventListener("change", batchCount);
  $("batch-mode").addEventListener("change", () => {
    const mode = $("batch-mode").value;
    $("batch-value").disabled = !mode;
    if (mode === "long") $("batch-value").value = 2048;
    if (mode === "percent") $("batch-value").value = 50;
  });
  $("batch-format").addEventListener("change", () => {
    const jpeg = $("batch-format").value === "jpeg";
    $("batch-mb").disabled = !jpeg;
    $("batch-quality").disabled = !jpeg;
  });
  $("batch-start").addEventListener("click", batchStart);
  $("batch-stop").addEventListener("click", () => {
    $("batch-now").textContent = t("Stopping after the photos under way…");
    invoke("batch_export_stop").catch(() => {});
  });
  $("batch-close").addEventListener("click", () => $("batch-dialog").close());
  $("batch-open").addEventListener("click", () => invoke("reveal_file", { path: batch.to }).catch(showError));
  tauri.event.listen("batch-export-progress", (e) => {
    const [done, total] = e.payload;
    $("batch-bar").max = total;
    $("batch-bar").value = done;
    const folder = batch.dirs[Math.min(done, batch.dirs.length - 1)];
    const vars = { done: batchNumber(done), total: batchNumber(total), folder };
    $("batch-now").textContent = folder ? t("{done} of {total} · {folder}", vars) : t("{done} of {total}", vars);
  });
}

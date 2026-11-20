// SPDX-License-Identifier: GPL-3.0-or-later
// Styles (Presets → Styles…): saved looks that fit themselves to each photo.
// A preset is fixed moves; a style keeps what Match look aims at and finds
// each photo's own settings to reach it, at an Amount. Made from reference
// photos (here, or Match look's Save as style…), applied to the open photo
// or the library's selection. Rust side: commands/styles.rs, amb_ops::style.
"use strict";

const styles = { list: [], chosen: "", busy: false };

// Match look's ticked parts as a style's focus: only tone, curve and detail
// is Tone, only colour parts is Colour, anything else the whole look.
function stylesFocusOf(parts) {
  if (parts.length && parts.every((p) => ["tone", "curve", "detail"].includes(p))) return "tone";
  if (parts.length && parts.every((p) => ["wb", "color", "grading", "hsl"].includes(p))) return "colour";
  return "all";
}

const STYLE_FOCUS_WORDS = { all: tKey("Whole look"), tone: tKey("Tone only"), colour: tKey("Colour only") };

// The photos a batch would take: the library's selection, or none.
function stylesSelection() {
  return libraryOn() ? selectedPhotos().filter((p) => !p.copy).map((p) => p.path) : [];
}

function stylesAmount() {
  return Number($("styles-amount").value) / 100;
}

function stylesRender() {
  const box = $("styles-list");
  $("styles-empty").hidden = styles.list.length > 0;
  box.hidden = styles.list.length === 0;
  box.replaceChildren(...styles.list.map((s) => {
    const name = el("b", { translate: "no" }, s.name);
    const from = t("{focus}, from {photos}", { focus: t(STYLE_FOCUS_WORDS[s.focus] ?? s.focus), photos: plural(s.references.length, "photo") });
    return el("button", {
      type: "button",
      class: `styles-item${s.name === styles.chosen ? " active" : ""}`,
      title: s.references.join(", "),
      onclick: () => {
        styles.chosen = s.name;
        stylesRender();
      },
    }, name, el("span", { class: "hint" }, from));
  }));
  const picked = styles.list.some((s) => s.name === styles.chosen);
  const selected = stylesSelection().length;
  $("styles-apply-one").disabled = styles.busy || !picked || !ui.info;
  $("styles-apply-many").disabled = styles.busy || !picked || !selected;
  $("styles-apply-many").textContent = selected ? t("Apply to {photos}", { photos: plural(selected, "selected photo") }) : t("Apply to selected photos");
  $("styles-delete").disabled = styles.busy || !picked;
  $("styles-new").disabled = styles.busy;
}

async function stylesLoad() {
  styles.list = await invoke("styles_list");
  if (!styles.list.some((s) => s.name === styles.chosen)) styles.chosen = styles.list[0]?.name ?? "";
  stylesRender();
}

async function stylesOpen() {
  $("styles-status").textContent = "";
  await stylesLoad();
  const dlg = $("styles-dialog");
  if (!dlg.open) dlg.showModal();
}

// A name from the person, then the style measured from `refs`.
async function stylesSaveFrom(refs, focus) {
  if (!refs.length) return;
  const name = await askName(t("New style"), t("A style keeps the look of these photos, not their settings. Name it."), "");
  if (!name) return;
  status(t("Measuring the look of {photos}…", { photos: plural(refs.length, "photo") }));
  const made = await invoke("style_save", { name, references: refs, focus });
  styles.chosen = made.name;
  status(t("Saved the style {name}", { name: made.name }));
  toast(t("Saved the style {name}. Presets → Styles… applies it.", { name: made.name }));
  if ($("styles-dialog").open) await stylesLoad();
}

async function stylesNew() {
  const refs = await invoke("pick_references");
  await stylesSaveFrom(refs, $("styles-focus").value);
}

// Key action: pick photos, name it, and it is saved (focus: the whole look).
async function stylesNewKey() {
  const refs = await invoke("pick_references");
  await stylesSaveFrom(refs, "all");
}

async function stylesApplyOne() {
  const line = $("styles-status");
  line.textContent = t("Finding this photo's settings…");
  await withBusy($("styles-apply-one"), line, async () => {
    const commands = await invoke("style_apply", { name: styles.chosen, amount: stylesAmount() });
    line.textContent = commands.length ? t("Applied {name} to this photo.", { name: styles.chosen }) : t("This photo already has the look.");
    if (commands.length) await run(commands, `Style: ${styles.chosen}`, { quiet: true });
  });
}

async function stylesApplyMany() {
  const photos = stylesSelection();
  const line = $("styles-status");
  styles.busy = true;
  stylesRender();
  line.textContent = t("Styling {photos}…", { photos: plural(photos.length, "photo") });
  const ok = await withBusy($("styles-apply-many"), line, async () => {
    const r = await invoke("styles_apply_batch", { name: styles.chosen, photos, amount: stylesAmount() });
    for (const path of r.changed) {
      photoChanged(path, { edited: true });
      refreshThumb(path);
    }
    if (r.state) {
      setState(r.state);
      requestPreview(false);
    }
    const done = t("Styled {photos}", { photos: plural(r.changed.length, "photo") });
    line.textContent = r.errors.length ? t("{text}; {n} could not be read: {error}", { text: done, n: r.errors.length, error: r.errors[0] }) : done;
  });
  styles.busy = false;
  stylesRender();
  return ok;
}

async function stylesDelete() {
  const name = styles.chosen;
  if (!name || !(await askYes(t("Delete the style {name}?", { name })))) return;
  await invoke("style_delete", { name });
  await stylesLoad();
}

function wireStyles() {
  $("btn-styles").addEventListener("click", () => stylesOpen().catch(showError));
  $("styles-new").addEventListener("click", () => stylesNew().catch((e) => { $("styles-status").textContent = t(String(e)); }));
  $("styles-apply-one").addEventListener("click", () => stylesApplyOne());
  $("styles-apply-many").addEventListener("click", () => stylesApplyMany());
  $("styles-delete").addEventListener("click", () => stylesDelete().catch((e) => { $("styles-status").textContent = t(String(e)); }));
  $("styles-close").addEventListener("click", () => $("styles-dialog").close());
  $("styles-amount").addEventListener("input", () => { $("styles-amount-value").textContent = $("styles-amount").value; });
  $("match-save-style").addEventListener("click", () => {
    const parts = [...$("match-dialog").querySelectorAll(".match-parts input:checked")].map((i) => i.value);
    stylesSaveFrom(styles.matchRefs ?? [], stylesFocusOf(parts)).catch(showError);
  });
  tauri.event.listen("style-progress", (e) => {
    const [done, total] = e.payload;
    $("styles-status").textContent = t("Styling… {done} of {total}", { done, total });
  });
}

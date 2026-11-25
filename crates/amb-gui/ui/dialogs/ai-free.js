// SPDX-License-Identifier: GPL-3.0-or-later
// AI-free mode in the editor: the switch (Settings → AI models, the
// `ai-free-toggle` key action), the AI controls turned off while it is on, and
// the one badge in the top bar: "AI-free" while the mode is on, else what the
// open photo's edit holds that a platform may label AI (`ai_label`, from
// `amb_ops::ai_label`), also shown in the export dialog; and the note of what
// the mode leaves out of the open photo. The mode itself is Rust's (`amb_presets::ai_free`, docs/design/ai-free.md);
// this only shows it. Which controls use a model is data: a `data-ai`
// attribute on the element, set from the registry's lists (`ai_free_info`,
// read first in `init`, before the controls that use it are built).
"use strict";

const aiFree = { on: false, mask: [], spot: [], left: [], label: null, observer: null };

const AI_FREE_OFF_TITLE = tKey("Off while AI-free mode is on");
const AI_LABEL_CAVEAT = tKey("Platforms may also run their own checks; this shows what is in the file.");

// Does this mask or spot kind (`subject`, `remove`, …) need a model? From the registry.
function aiFreeUses(kind) {
  return aiFree.mask.includes(kind) || aiFree.spot.includes(kind);
}

// Turn one marked control off, or give it back as it was. A control that was
// off for another reason (no photo open) goes back to off.
function aiFreeLock(node, lock) {
  if (lock) {
    if (node.dataset.aiWas == null) {
      node.dataset.aiWas = node.disabled ? "1" : "0";
      node.dataset.aiTitle = node.title;
    } else if (!node.disabled) node.dataset.aiWas = "0";
    // Only when it differs: setting the same value still fires the observer.
    if (!node.disabled) node.disabled = true;
    const title = t(AI_FREE_OFF_TITLE);
    if (node.title !== title) node.title = title;
  } else if (node.dataset.aiWas != null) {
    node.disabled = node.dataset.aiWas === "1";
    node.title = node.dataset.aiTitle;
    delete node.dataset.aiWas;
    delete node.dataset.aiTitle;
  }
}

// The one place that turns the marked controls off and on, and shows the mode.
function aiFreeApply() {
  $("ai-free-switch").checked = aiFree.on;
  for (const node of document.querySelectorAll("[data-ai]")) aiFreeLock(node, aiFree.on);
}

// The badge's state, its words and the reasons for them, from the last `ai_label`.
function aiFreeVerdict() {
  const parts = (list) => list.map((p) => t(p)).join(", ");
  const lines = [];
  let state = "clear";
  let text = t("No AI marks");
  if (aiFree.on) {
    state = "free";
    text = t("AI-free");
    lines.push(t("AI-free mode is on: no AI model touches your photos. Click to change it in Settings."));
    if (aiFree.left.length) lines.push(t("AI-free mode leaves out: {parts}.", { parts: parts(aiFree.left) }));
  } else if (aiFree.label?.verdict === "ai_in_edit") {
    state = "edit";
    text = t("AI in edit");
    lines.push(t("The edit holds AI parts that the export carries: {parts}.", { parts: parts(aiFree.label.edit_parts) }));
  } else lines.push(t("The edit holds no AI parts."));
  lines.push(t(AI_LABEL_CAVEAT));
  return { state, text, lines };
}

// Show or hide the badge. Written only when it differs: the observer below watches the page.
function aiFreeBadge() {
  const badge = $("ai-free-badge");
  badge.hidden = !(aiFree.on || (ui.info && aiFree.label));
  if (badge.hidden) return;
  const v = aiFreeVerdict();
  if (badge.dataset.state !== v.state) badge.dataset.state = v.state;
  if (badge.textContent !== v.text) badge.textContent = v.text;
  const title = v.lines.join("\n");
  if (badge.title !== title) badge.title = title;
}

// The export dialog's line: what the file will carry, before it is written.
function aiFreeExportNote(show) {
  const note = $("export-ai-label");
  note.hidden = !show || !ui.info || !aiFree.label;
  if (note.hidden) return;
  const v = aiFreeVerdict();
  note.dataset.state = v.state;
  // With the mode on, the first line is about the Settings click.
  note.textContent = `${v.text}. ${v.lines.slice(aiFree.on ? 1 : 0).join(" ")}`;
}

// What the open photo's edit holds, for the badge and for what the mode leaves out.
async function aiFreeNote() {
  const note = $("ai-free-note");
  aiFree.label = ui.info ? await invoke("ai_label", { path: ui.info.path }).catch(() => null) : null;
  aiFree.left = aiFree.label?.left_out ?? [];
  aiFreeBadge();
  note.hidden = !aiFree.left.length;
  note.textContent = aiFree.left.length ? t("AI-free mode leaves out: {parts}.", { parts: aiFree.left.map((p) => t(p)).join(", ") }) : "";
}

async function aiFreeSet(on) {
  try {
    aiFree.on = await invoke("ai_free_set", { on });
  } catch (e) {
    showError(e);
  }
  aiFreeApply();
  await aiFreeNote();
  if (mk.on) renderMaskPanel();
  if (ui.info) requestPreview(false);
  syncMenu();
  toast(aiFree.on ? t("AI-free mode is on.") : t("AI-free mode is off."));
}

const aiFreeToggle = () => aiFreeSet(!aiFree.on);

async function aiFreeLoad() {
  const info = await invoke("ai_free_info");
  aiFree.on = info.on;
  aiFree.mask = info.mask_kinds;
  aiFree.spot = info.spot_kinds;
  for (const b of document.querySelectorAll("#spot-kind button")) if (aiFree.spot.includes(b.dataset.kind)) b.dataset.ai = "";
  aiFreeApply();
  aiFreeBadge();
}

function wireAiFree() {
  $("ai-free-switch").addEventListener("change", (e) => aiFreeSet(e.target.checked));
  $("ai-free-badge").addEventListener("click", () => settingsFlow("models").catch(showError));
  // Controls built later (mask kinds, tool tiles) are turned off as they appear.
  aiFree.observer = new MutationObserver(() => aiFree.on && aiFreeApply());
  aiFree.observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled"] });
}

// SPDX-License-Identifier: GPL-3.0-or-later
// AI-free mode in the editor: the switch (Settings → AI models, the
// `ai-free-toggle` key action), the badge in the top bar, the AI controls
// turned off while it is on, and the note of what it leaves out of the open
// photo. The mode itself is Rust's (`amb_presets::ai_free`, docs/design/ai-free.md);
// this only shows it. Which controls use a model is data: a `data-ai`
// attribute on the element, set from the registry's lists (`ai_free_info`,
// read first in `init`, before the controls that use it are built).
"use strict";

const aiFree = { on: false, mask: [], spot: [], left: [], observer: null };

const AI_FREE_OFF_TITLE = tKey("Off while AI-free mode is on");

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
  $("ai-free-badge").hidden = !aiFree.on;
  $("ai-free-switch").checked = aiFree.on;
  for (const node of document.querySelectorAll("[data-ai]")) aiFreeLock(node, aiFree.on);
}

// What the open photo's edit holds that the mode leaves out.
async function aiFreeNote() {
  const note = $("ai-free-note");
  aiFree.left = aiFree.on && ui.info ? await invoke("ai_free_left_out").catch(() => []) : [];
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
}

function wireAiFree() {
  $("ai-free-switch").addEventListener("change", (e) => aiFreeSet(e.target.checked));
  $("ai-free-badge").addEventListener("click", () => settingsFlow("models").catch(showError));
  // Controls built later (mask kinds, tool tiles) are turned off as they appear.
  aiFree.observer = new MutationObserver(() => aiFree.on && aiFreeApply());
  aiFree.observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled"] });
}

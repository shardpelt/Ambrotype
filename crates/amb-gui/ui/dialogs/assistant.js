// SPDX-License-Identifier: GPL-3.0-or-later
// The assistant: the user's own AI model edits the open photo
// (src/assistant.rs). Its edits are ordinary commands, so they show in the
// history and Undo takes them back. The conversation starts again for each
// photo.
"use strict";

const asst = {
  wired: false,
  photo: null,   // the photo the conversation is about
  history: [],   // the model's messages, sent back with each request
  busy: false,
  tokens: { in: 0, out: 0 },
  settings: null,
};

const ASSISTANT_URLS = { anthropic: "https://api.anthropic.com", open_ai: "http://localhost:11434/v1" };

async function assistantFlow() {
  if (!ui.info) return toast(t("Open a photo first."), true);
  wireAssistant();
  const dlg = $("assistant-dialog");
  if (asst.photo !== ui.info.path) {
    asst.photo = ui.info.path;
    asst.history = [];
    asst.tokens = { in: 0, out: 0 };
    $("assistant-log").replaceChildren();
    assistantStatus();
  }
  try {
    asst.settings = await invoke("assistant_settings");
  } catch (e) {
    showError(e);
  }
  const ready = asst.settings?.provider.model && (asst.settings.has_key || asst.settings.provider.kind !== "anthropic");
  fillAssistantSettings();
  $("assistant-settings").hidden = Boolean(ready);
  if (!dlg.open) dlg.show();
  (ready ? $("assistant-input") : $("assistant-model")).focus();
}

function wireAssistant() {
  if (asst.wired) return;
  asst.wired = true;
  $("assistant-close").addEventListener("click", () => $("assistant-dialog").close());
  $("assistant-log-open").addEventListener("click", () => invoke("open_assistant_log").catch(showError));
  $("assistant-gear").addEventListener("click", () => {
    fillAssistantSettings();
    $("assistant-settings").hidden = !$("assistant-settings").hidden;
  });
  $("assistant-kind").addEventListener("change", () => {
    $("assistant-url").value = "";
    assistantPlaceholders();
  });
  $("assistant-settings").addEventListener("submit", (e) => {
    e.preventDefault();
    saveAssistantSettings();
  });
  $("assistant-form").addEventListener("submit", (e) => {
    e.preventDefault();
    askAssistant();
  });
  const input = $("assistant-input");
  input.addEventListener("keydown", (e) => {
    // Enter sends, Shift+Enter is a new line; keys typed here aren't shortcuts.
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      askAssistant();
    }
    if (e.key === "Escape") $("assistant-dialog").close();
    e.stopPropagation();
  });
  for (const id of ["assistant-url", "assistant-model", "assistant-key"]) $(id).addEventListener("keydown", (e) => e.stopPropagation());
  // Remembered on this computer; off until the user ticks it.
  const see = $("assistant-see");
  see.checked = store.text("amb.assistant.see") === "1";
  see.addEventListener("change", () => {
    store.setText("amb.assistant.see", see.checked ? "1" : "");
  });
}

function fillAssistantSettings() {
  const p = asst.settings?.provider ?? { kind: "anthropic", base_url: "", model: "" };
  $("assistant-kind").value = p.kind;
  $("assistant-url").value = p.base_url;
  $("assistant-model").value = p.model;
  $("assistant-key").value = "";
  assistantPlaceholders();
}

function assistantPlaceholders() {
  const kind = $("assistant-kind").value;
  $("assistant-url").placeholder = ASSISTANT_URLS[kind];
  const saved = asst.settings?.has_key && asst.settings.provider.kind === kind;
  $("assistant-key").placeholder = saved ? t("Saved (type a new one to replace it)")
    : kind === "anthropic" ? t("Your API key") : t("Only if the server needs one");
}

async function saveAssistantSettings() {
  const provider = {
    kind: $("assistant-kind").value,
    base_url: $("assistant-url").value.trim(),
    model: $("assistant-model").value.trim(),
  };
  const key = $("assistant-key").value.trim();
  try {
    const note = await invoke("assistant_save", { provider, key: key || null });
    if (note) toast(note, true);
    asst.settings = await invoke("assistant_settings");
    $("assistant-settings").hidden = true;
    $("assistant-input").focus();
  } catch (e) {
    showError(e);
  }
}

function assistantMessage(role, text) {
  const div = el("div", { class: `msg ${role}` }, text);
  const log = $("assistant-log");
  log.append(div);
  log.scrollTop = log.scrollHeight;
  return div;
}

function assistantStatus(text) {
  const tokens = asst.tokens;
  const used = tokens.in || tokens.out
    ? t("{total} tokens so far ({in} in, {out} out).", { total: tokens.in + tokens.out, in: tokens.in, out: tokens.out })
    : "";
  $("assistant-status").textContent = [text, used].filter(Boolean).join(" ");
}

async function askAssistant() {
  const input = $("assistant-input");
  const request = input.value.trim();
  if (!request || asst.busy) return;
  if (!ui.info || ui.info.path !== asst.photo) return assistantFlow();
  asst.busy = true;
  $("assistant-send").disabled = true;
  input.value = "";
  assistantMessage("user", request);
  assistantStatus(t("Thinking…"));
  try {
    const r = await invoke("assistant_ask", { history: asst.history, request, see: $("assistant-see").checked });
    asst.history = r.history;
    asst.tokens.in += r.tokens_in;
    asst.tokens.out += r.tokens_out;
    let last = null;
    for (const m of r.shown) last = assistantMessage(m.role, m.text);
    if (!r.shown.length) last = assistantMessage("app", t("No answer."));
    if (r.edits) {
      setState(r.state);
      requestPreview(false);
      // Undo takes back this request's edits, if nothing came after them.
      const steps = r.edits;
      const done = r.state.history.length;
      const undo = el("button", {
        type: "button",
        class: "small",
      }, steps > 1 ? t("Undo these {n} edits", { n: steps }) : t("Undo"));
      undo.addEventListener("click", async () => {
        if (ui.state?.history.length !== done) return toast(t("Other edits came after these; use the History panel."), true);
        await run(Array.from({ length: steps }, () => ({ cmd: "undo" })));
        undo.disabled = true;
        undo.textContent = t("Undone");
      });
      last.append(el("br"), undo);
    }
    assistantStatus();
  } catch (e) {
    assistantMessage("app", String(e));
    assistantStatus();
  } finally {
    asst.busy = false;
    $("assistant-send").disabled = false;
    input.focus();
  }
}

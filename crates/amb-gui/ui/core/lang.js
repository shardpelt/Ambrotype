// SPDX-License-Identifier: GPL-3.0-or-later
// The chosen language (docs/design/language.md): loading its texts, and
// swapping the markup's text for them. `t` and `plural` are in core.js.

const lang = {
  code: "en",       // the language shown ("en" has no file: the text is English)
  texts: null,      // English text to translation; null in English
  rules: null,      // Intl.PluralRules for the language
  plurals: [],      // its plural categories, in the order a file lists the forms
  list: [],         // the languages there are files for: { code, name }
};

// What a person chose: "system", "en" or a language's code.
function langChoice() {
  return store.text("amb.lang", "system");
}

// The code to show for a choice: "system" is the first language that matches the system's.
function langResolve(choice) {
  if (choice === "zz") return "zz";
  if (choice !== "system") return lang.list.some((l) => l.code === choice) ? choice : "en";
  const wanted = (navigator.languages ?? [navigator.language]).map((l) => String(l).toLowerCase().split("-")[0]);
  return wanted.find((w) => lang.list.some((l) => l.code === w)) ?? "en";
}

// Load a language and show it. `zz` is the test language that marks every text.
async function langSet(code) {
  lang.code = code;
  lang.texts = code === "en" ? null : await invoke("language_texts", { code });
  lang.rules = new Intl.PluralRules(code === "zz" ? "en" : code);
  lang.plurals = lang.rules.resolvedOptions().pluralCategories;
  document.documentElement.lang = code === "zz" ? "en" : code;
  translateDom(document.body);
}

// Texts of the markup that no language changes: scripts, styles, typed text.
const LANG_SKIP = new Set(["SCRIPT", "STYLE", "TEXTAREA", "CODE", "KBD"]);
const LANG_ATTRS = ["title", "placeholder", "aria-label"];

// Swap the text, `title`, `placeholder` and `aria-label` under `root` for the
// language's, when its text is in the table. Whitespace round the text stays.
// In English it does nothing.
function translateDom(root) {
  if (!lang.texts) return;
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) {
    if (LANG_SKIP.has(n.parentElement?.tagName)) continue;
    const text = n.nodeValue.replace(/\s+/g, " ").trim();
    const out = text && lang.texts[text];
    if (out) n.nodeValue = n.nodeValue.replace(/\S(?:[\s\S]*\S)?/, () => out);
  }
  for (const e of root.querySelectorAll("[title],[placeholder],[aria-label]")) {
    for (const a of LANG_ATTRS) {
      const out = e.hasAttribute(a) && lang.texts[e.getAttribute(a).trim()];
      if (out) e.setAttribute(a, out);
    }
  }
}

// The texts on screen that no language has been through: the `zz` language
// wraps each one in ⟦ ⟧, so what lacks them is not in the table (tests/ui/language.js).
function langUnmarked(root = document.body) {
  const found = [];
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) {
    if (LANG_SKIP.has(n.parentElement?.tagName)) continue;
    const text = n.nodeValue.trim();
    if (/\p{L}/u.test(text) && !text.startsWith("⟦")) found.push(text);
  }
  for (const e of root.querySelectorAll("[title],[placeholder],[aria-label]")) {
    for (const a of LANG_ATTRS) {
      const text = e.getAttribute(a)?.trim();
      if (text && /\p{L}/u.test(text) && !text.startsWith("⟦")) found.push(text);
    }
  }
  return found;
}

// Settings → Appearance → Language: shown once there is a file besides English.
async function wireLanguage() {
  lang.list = await invoke("language_list");
  const pick = $("settings-language");
  const row = $("settings-language-row");
  row.hidden = lang.list.length === 0;
  for (const l of lang.list) pick.append(el("option", { value: l.code }, l.name));
  pick.value = langChoice();
  if (pick.value !== langChoice()) pick.value = "system";
  await langSet(langResolve(langChoice()));
  pick.addEventListener("change", async () => {
    store.setText("amb.lang", pick.value);
    await langSet(langResolve(pick.value));
  });
}

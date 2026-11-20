// SPDX-License-Identifier: GPL-3.0-or-later
// The chosen language (docs/design/language.md): loading its texts, and
// swapping the markup's text for them. `t` and `plural` are in core.js.

const lang = {
  code: "en",       // the language shown ("en" has no file: the text is English)
  texts: null,      // English text to translation; null in English
  rules: null,      // Intl.PluralRules for the language
  plurals: [],      // its plural categories, in the order a file lists the forms
  list: [],         // the languages there are files for: { code, name }
  swapped: false,   // whether translateDom ever changed the markup
};

// The plural categories in the order a file lists the forms (CLDR's; the
// order Intl gives them in differs between webviews).
const LANG_PLURAL_ORDER = ["zero", "one", "two", "few", "many", "other"];

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
// If its texts can't be loaded, the language shown stays as it was.
async function langSet(code) {
  const texts = code === "en" ? null : await invoke("language_texts", { code });
  lang.code = code;
  lang.texts = texts;
  lang.rules = new Intl.PluralRules(code === "zz" ? "en" : code);
  const has = lang.rules.resolvedOptions().pluralCategories;
  lang.plurals = LANG_PLURAL_ORDER.filter((c) => has.includes(c));
  document.documentElement.lang = code === "zz" ? "en" : code;
  translateDom(document.body);
}

// Texts of the markup that no language changes: scripts, styles, typed text.
const LANG_SKIP = new Set(["SCRIPT", "STYLE", "TEXTAREA", "CODE", "KBD"]);
const LANG_ATTRS = ["title", "placeholder", "aria-label"];
// People's own words (preset names and groups) sit in translate="no":
// no language changes them, and the count of texts left leaves them out.
const langOwn = (e) => Boolean(e?.closest('[translate="no"]'));

// What translateDom showed, to change back: text node -> { en, shown };
// element -> { [attribute]: { en, shown } }.
const LANG_SHOWN = new WeakMap();

// The English behind a text: what translateDom showed is changed back; a text
// a script set since is the English now.
function langEnglish(now, saved) {
  return saved && saved.shown === now ? saved.en : now;
}

// Swap the text, `title`, `placeholder` and `aria-label` under `root` for the
// language's, when its text is in the table, and back to English for a text
// it isn't in (after a switch of language). Whitespace round the text stays.
// In English, with nothing swapped before, it does nothing.
function translateDom(root) {
  if (!lang.texts && !lang.swapped) return;
  const look = (en) => {
    const key = en.replace(/\s+/g, " ").trim();
    return (key && lang.texts?.[key]) || null;
  };
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) {
    if (LANG_SKIP.has(n.parentElement?.tagName) || langOwn(n.parentElement)) continue;
    const en = langEnglish(n.nodeValue, LANG_SHOWN.get(n));
    const out = look(en);
    const value = out ? en.replace(/\S(?:[\s\S]*\S)?/, () => out) : en;
    if (value !== n.nodeValue) n.nodeValue = value;
    if (out) LANG_SHOWN.set(n, { en, shown: value });
    else LANG_SHOWN.delete(n);
  }
  for (const e of root.querySelectorAll("[title],[placeholder],[aria-label]")) {
    if (langOwn(e)) continue;
    const saved = LANG_SHOWN.get(e) ?? {};
    for (const a of LANG_ATTRS) {
      if (!e.hasAttribute(a)) continue;
      const en = langEnglish(e.getAttribute(a), saved[a]);
      const out = look(en);
      const value = out ?? en;
      if (value !== e.getAttribute(a)) e.setAttribute(a, value);
      if (out) saved[a] = { en, shown: value };
      else delete saved[a];
    }
    if (Object.keys(saved).length) LANG_SHOWN.set(e, saved);
    else LANG_SHOWN.delete(e);
  }
  if (lang.texts) lang.swapped = true;
}

// The texts on screen that no language has been through: the `zz` language
// wraps each one in ⟦ ⟧, so what lacks them is not in the table (tests/ui/language.js).
function langUnmarked(root = document.body) {
  const found = [];
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) {
    if (LANG_SKIP.has(n.parentElement?.tagName) || langOwn(n.parentElement)) continue;
    const text = n.nodeValue.trim();
    if (/\p{L}/u.test(text) && !text.startsWith("⟦")) found.push(text);
  }
  for (const e of root.querySelectorAll("[title],[placeholder],[aria-label]")) {
    if (langOwn(e)) continue;
    for (const a of LANG_ATTRS) {
      const text = e.getAttribute(a)?.trim();
      if (text && /\p{L}/u.test(text) && !text.startsWith("⟦")) found.push(text);
    }
  }
  return found;
}

// Settings → General → Language: shown once there is a file besides English.
async function wireLanguage() {
  lang.list = await invoke("language_list");
  const pick = $("settings-language");
  const row = $("settings-language-row");
  row.hidden = lang.list.length === 0;
  for (const l of lang.list) pick.append(el("option", { value: l.code }, l.name));
  pick.value = langChoice();
  if (pick.value !== langChoice()) pick.value = "system";
  // A language that won't load leaves the app in English, not without a start.
  await langSet(langResolve(langChoice())).catch(showError);
  pick.addEventListener("change", () => {
    store.setText("amb.lang", pick.value);
    langSet(langResolve(pick.value)).catch(showError);
  });
}

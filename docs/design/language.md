# One table for the app's words

Decided 2026-11-13, not built yet. The roadmap row is `language-table` in [ROADMAP.md](../ROADMAP.md).

## Why

- The same words are typed in several places (an action's label, its menu entry, its tooltip, a toast) and drift apart.
- The app can't be translated.
- Checking the wording means reading every script.

How much text there is (counted 2026-11-13; count again with the greps in "The table" below): the markup in `crates/amb-gui/ui/index.html` has about 900 pieces of text, 317 tooltips, 56 placeholders and 45 screen-reader labels. The scripts have about 540 pieces of text written as plain strings (163 toasts, 201 labels, 103 `textContent`, 71 tooltips), and more built with `${…}`.

Three kinds of text are tables already: the settings' labels and descriptions (`crates/amb-protocol/data/descriptions.json`), the lessons and the tips.

## The decision: the English text is the key

As in Laravel's JSON language files: the code says `t("Moved to the trash")`, and `nl.json` holds `"Moved to the trash": "Naar de prullenbak verplaatst"`. There is no `en.json` to keep, and no invented key such as `presets.trash_toast`.

Why this and not named keys:
- The code stays readable, and a search for the words on screen finds the code.
- Nothing has to be pulled out before it works. Text nobody wrapped yet just stays English.
- A new feature can't forget a key file. Its author writes `t("…")` and is done.

What it costs:
- Changing an English text orphans its translations. The generated table shows them, and a missing translation falls back to English.
- One English text with two meanings ("Open" the verb, "Open" the state) needs telling apart. Write it `t("Open##verb")`: everything from `##` is left off on screen. Use it only when a translation needs it.

## The parts

1. **`t(text, vars)`** in `crates/amb-gui/ui/core/core.js`. Looks the text up in the loaded language; puts `vars` in place of `{name}`; gives the text back when there is no translation. `t("Copied as “{name}”", { name })`.
2. **`plural(n, one, many)`** (it exists) looks up `"{n} photo|{n} photos"`. A language with more forms lists them all, split by `|`, in the order `Intl.PluralRules` names them for that language.
3. **Markup needs no keys.** At startup, `translateDom(document.body)` swaps each piece of text, `title`, `placeholder` and `aria-label` whose text is in the table. In English it does nothing.
4. **Words built in a script** are wrapped by whoever writes them: `t("…")`. Text from a file or a person (a file name, a preset's name, a keyword) is never looked up.
5. **Lists of words that are data** (`KEY_ACTIONS` labels, the menu bar, the tips) are looked up where they are shown: `t(action.label)`.
6. **Language files:** `crates/amb-gui/data/lang/<code>.json`, built in. Later a hub item kind, so translations are shared like themes ([recipes.md](../recipes.md), "a hub item kind"). Settings → Appearance → Language: System, English, and each language there is a file for.
7. **Dates and numbers** use `Intl` with the chosen language.

## The table

`tools/lang-table.py` reads the scripts (`t("…")`, `plural(…)`, the labels of data lists) and `index.html` and writes `crates/amb-gui/data/lang/keys.json`: every text, with the places it is used. It is generated: a test fails when it is stale, as for the man page.

The tool also prints near-duplicates: texts that differ only by case, a trailing full stop, or `...` against `…`. Fixing those is the first use of the table.

## Knowing what is left

A made-up language `zz` turns every text in the table into `⟦text⟧`. A UI test runs the app in `zz` and counts the texts on screen without brackets: those are not through the table yet. That count may only fall (`crates/amb-gui/src/checks.rs` holds the others).

## Words that come from Rust

- **Settings' labels and descriptions:** a `descriptions.<code>.json` beside the English one.
- **Errors and reports from editor commands** are English sentences today. First, `showError` passes them through `t()`: a fixed sentence translates, one with a file name in it does not. After card B11 in [cleanup.md](../cleanup.md) (one error type), an error carries a code and its values, and translates whole.
- **`amb`** stays English.

## Not translated

File names, preset names, setting names (`Exposure2012`), `amb`'s output, the docs. Right-to-left languages: not now.

## In four steps

1. **The base** (medium tier): parts 1 to 3, 6 with English only, the table and its test, `zz` and its count. Nothing changes on screen. One line in [codestyle.md](../codestyle.md): words built in a script go through `t()`.
2. **The sweep** (small or medium, one folder of `crates/amb-gui/ui/` per commit): wrap the scripts' words; the `zz` count falls to zero.
3. **Rust's words:** the settings' texts per language; errors by code (after B11).
4. **The first translation,** and languages from the hub. The owner picks the language; a small model drafts, a person checks.

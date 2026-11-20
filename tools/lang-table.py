#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The table of the editor's words (docs/design/language.md).

    python3 tools/lang-table.py          write crates/amb-gui/data/lang/keys.json
    python3 tools/lang-table.py --check  fail when keys.json is stale (a test runs this)

Reads `t("…")`, `tKey("…")`, `plural(n, "one", "many")` and the labels of the data lists in
the scripts, and the text, `title`, `placeholder` and `aria-label` of
`index.html`. Writes each text with the files that use it, and prints texts
that differ only by case, a trailing full stop or `...` against `…`.
"""
import html.parser
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UI = os.path.join(ROOT, "crates/amb-gui/ui")
OUT = os.path.join(ROOT, "crates/amb-gui/data/lang/keys.json")

# Scripts whose lists of words are data: `label: "…"` and `hint: "…"` there are shown through t().
LIST_FILES = {"core/keys.js", "core/menu.js", "core/tasks.js", "tools/toolblocks.js"}
LIST_FIELDS = ("label", "hint")

STRING = r'"((?:\\.|[^"\\\n])*)"|\'((?:\\.|[^\'\\\n])*)\'|`((?:\\.|[^`\\$])*)`'
T_CALL = re.compile(r"(?<![\w.$])(?:t|tKey)\(\s*(?:" + STRING + r")")
PLURAL_CALL = re.compile(
    r"(?<![\w.$])plural\([^,()]+,\s*(?:\"([^\"\\\n]*)\"|'([^'\\\n]*)')(?:\s*,\s*(?:\"([^\"\\\n]*)\"|'([^'\\\n]*)'))?"
)
LIST_FIELD = re.compile(r"\b(?:" + "|".join(LIST_FIELDS) + r"):\s*(?:" + STRING + r")")
ATTRS = ("title", "placeholder", "aria-label")
SKIP_TAGS = {"script", "style", "textarea", "code", "kbd"}


def unescape(s):
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), s)


def first(m, n=3):
    return unescape(next(g for g in m.groups()[:n] if g is not None))


class Page(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.found = []
        self.skip = []

    def handle_starttag(self, tag, attrs):
        if tag in SKIP_TAGS:
            self.skip.append(tag)
        for k, v in attrs:
            if k in ATTRS and v:
                self.found.append(" ".join(v.split()))

    def handle_endtag(self, tag):
        if self.skip and self.skip[-1] == tag:
            self.skip.pop()

    def handle_data(self, data):
        if not self.skip:
            self.found.append(" ".join(data.split()))


def keep(text):
    return bool(re.search(r"[^\W\d_]", text))


def collect():
    keys = {}

    def add(text, where):
        if keep(text):
            keys.setdefault(text, set()).add(where)

    page = Page()
    page.feed(open(os.path.join(UI, "index.html"), encoding="utf-8").read())
    for text in page.found:
        add(text, "index.html")
    for dirpath, _, names in os.walk(UI):
        for name in names:
            if not name.endswith(".js"):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, UI).replace(os.sep, "/")
            text = open(path, encoding="utf-8").read()
            for m in T_CALL.finditer(text):
                add(first(m), rel)
            for m in PLURAL_CALL.finditer(text):
                one = m.group(1) if m.group(1) is not None else m.group(2)
                many = m.group(3) if m.group(3) is not None else m.group(4)
                many = many if many is not None else one + "s"
                add(f"{{n}} {one}|{{n}} {many}", rel)
            if rel in LIST_FILES:
                for m in LIST_FIELD.finditer(text):
                    add(first(m), rel)
    return {k: sorted(keys[k]) for k in sorted(keys)}


def near_duplicates(keys):
    groups = {}
    for k in keys:
        norm = k.replace("...", "…").rstrip(".").lower()
        groups.setdefault(norm, []).append(k)
    return [g for g in groups.values() if len(g) > 1]


def main():
    keys = collect()
    text = json.dumps(keys, ensure_ascii=False, indent=1) + "\n"
    if "--check" in sys.argv:
        have = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
        if have != text:
            sys.exit("crates/amb-gui/data/lang/keys.json is stale: run `python3 tools/lang-table.py`")
        return
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    open(OUT, "w", encoding="utf-8").write(text)
    print(f"{len(keys)} texts in {os.path.relpath(OUT, ROOT)}")
    dupes = near_duplicates(keys)
    for g in dupes:
        print("near-duplicate:", " | ".join(repr(k) for k in g))
    print(f"{len(dupes)} groups of near-duplicates")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build index.json from the look files in looks/ (run from the hub's root),
and check the plugins sent in under plugins/.

    python3 tools/build_index.py          # write index.json
    python3 tools/build_index.py --check  # exit 1 if index.json is out of date (CI)

Each file becomes one item. Its id is the file name, and the app checks the
file's sha256 against the index before installing it. Validate the looks
themselves with the Ambrotype app: `ambrotype --check-look looks/*/*.json`.

Plugins (plugins/<id>/) aren't in the index yet. Each needs a plugin.json
whose id is its folder name, its plugin.wasm, and its Rust source, so a
reviewer can read the code and rebuild the module: `amb plugin build
plugins/<id>`, then `amb plugin check plugins/<id>`.
"""
import hashlib
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
KINDS = {"layouts": ("layout", "ambrotype-layout/1"), "themes": ("theme", "ambrotype-theme/1")}


def build():
    items, problems = [], []
    for folder, (kind, fmt) in KINDS.items():
        for path in sorted((ROOT / "looks" / folder).glob("*.json")):
            rel = path.relative_to(ROOT).as_posix()
            if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,62}", path.stem):
                problems.append(f"{rel}: use a lower-case file name with dashes (e.g. warm-dark.json)")
                continue
            raw = path.read_bytes()
            try:
                data = json.loads(raw)
            except json.JSONDecodeError as e:
                problems.append(f"{rel}: not valid JSON: {e}")
                continue
            if data.get("format") != fmt:
                problems.append(f'{rel}: "format" must be "{fmt}" in looks/{folder}/')
                continue
            if not str(data.get("name", "")).strip():
                problems.append(f'{rel}: needs a "name"')
                continue
            items.append({
                "id": f"{kind}-{path.stem}",
                "kind": kind,
                "name": data["name"],
                "author": data.get("author", ""),
                "description": data.get("description", ""),
                "license": data.get("license", "CC0-1.0"),
                "path": rel,
                "sha256": hashlib.sha256(raw).hexdigest(),
            })
    problems += check_plugins()
    return {"format": "ambrotype-hub/1", "items": items}, problems


def check_plugins():
    problems = []
    for folder in sorted(p for p in (ROOT / "plugins").glob("*") if p.is_dir()):
        rel = folder.relative_to(ROOT).as_posix()
        try:
            data = json.loads((folder / "plugin.json").read_bytes())
        except (OSError, json.JSONDecodeError) as e:
            problems.append(f"{rel}: needs a valid plugin.json ({e})")
            continue
        if data.get("format") != "ambrotype-plugin/1":
            problems.append(f'{rel}/plugin.json: "format" must be "ambrotype-plugin/1"')
        if data.get("id") != folder.name:
            problems.append(f'{rel}/plugin.json: "id" must be the folder name, "{folder.name}"')
        if not (folder / data.get("wasm", "plugin.wasm")).is_file():
            problems.append(f"{rel}: the built module (plugin.wasm) is missing")
        if not any(folder.rglob("*.rs")):
            problems.append(f"{rel}: the source is missing (reviewers rebuild plugin.wasm from it)")
    return problems


def main():
    index, problems = build()
    for p in problems:
        print(p, file=sys.stderr)
    text = json.dumps(index, indent=2, ensure_ascii=False) + "\n"
    target = ROOT / "index.json"
    if "--check" in sys.argv:
        current = target.read_text(encoding="utf-8") if target.exists() else ""
        if current != text:
            print("index.json is out of date: run python3 tools/build_index.py", file=sys.stderr)
            sys.exit(1)
    else:
        target.write_text(text, encoding="utf-8")
        print(f"index.json: {len(index['items'])} looks")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()

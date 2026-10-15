#!/usr/bin/env python3
"""Build index.json from the look files in looks/ (run from the hub's root),
and check the plugins sent in under plugins/.

    python3 tools/build_index.py          # write index.json
    python3 tools/build_index.py --check  # exit 1 if index.json is out of date (CI)

Each file becomes one item (looks/, collages/ for collage templates and
sizes/ for size profiles). Its id is the file name, and the app checks the
file's sha256 against the index before installing it. Validate the looks
themselves with the Ambrotype app: `ambrotype --check-look looks/*/*.json`.

Develop presets (presets/<name>.xmp, Lightroom's XMP preset format) are
listed as "preset-<name>".

Plugins (plugins/<id>/) are listed too, as "plugin-<id>": the index holds
the sha256 of plugin.json and of its module, and the app checks both. Each
needs a plugin.json whose id is its folder name, its plugin.wasm, and its
Rust source, so a reviewer can read the code and rebuild the module: `amb
plugin build plugins/<id>`, then `amb plugin check plugins/<id>`.
"""
import hashlib
import html
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
KINDS = {"layouts": ("layout", "ambrotype-layout/1"), "themes": ("theme", "ambrotype-theme/1")}
# Other data files, each in its own top-level folder.
DATA = {"collages": ("collage", "ambrotype-collage/1"), "sizes": ("sizes", "ambrotype-sizes/1")}


def build():
    items, problems = [], []
    folders = [(ROOT / "looks" / f, f, k) for f, k in KINDS.items()]
    folders += [(ROOT / f, f, k) for f, k in DATA.items()]
    for base, folder, (kind, fmt) in folders:
        for path in sorted(base.glob("*.json")):
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
                problems.append(f'{rel}: "format" must be "{fmt}" in {base.relative_to(ROOT).as_posix()}/')
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
    preset_items, preset_problems = check_presets()
    plugin_items, plugin_problems = check_plugins()
    items += preset_items + plugin_items
    return {"format": "ambrotype-hub/1", "items": items}, problems + preset_problems + plugin_problems


def xmp_text(raw, name):
    """A preset's crs:<name>, as an attribute or a language alternative."""
    text = raw.decode("utf-8", "replace")
    m = re.search(rf'crs:{name}="([^"]*)"', text) or re.search(
        rf"<crs:{name}>.*?<rdf:li[^>]*>(.*?)</rdf:li>", text, re.S)
    return html.unescape(m.group(1)).strip() if m else ""


def check_presets():
    items, problems = [], []
    for path in sorted((ROOT / "presets").glob("*.xmp")):
        rel = path.relative_to(ROOT).as_posix()
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,62}", path.stem):
            problems.append(f"{rel}: use a lower-case file name with dashes (e.g. warm-film.xmp)")
            continue
        raw = path.read_bytes()
        if b"crs:PresetType" not in raw and b"<crs:PresetType" not in raw:
            problems.append(f"{rel}: not a develop preset (no crs:PresetType); export it from Lightroom or Ambrotype")
            continue
        items.append({
            "id": f"preset-{path.stem}",
            "kind": "preset",
            "name": xmp_text(raw, "Name") or path.stem,
            "group": xmp_text(raw, "Group"),
            "author": xmp_text(raw, "Author"),
            "description": xmp_text(raw, "Description"),
            "license": xmp_text(raw, "License") or "CC0-1.0",
            "path": rel,
            "sha256": hashlib.sha256(raw).hexdigest(),
        })
    return items, problems


def check_plugins():
    items, problems = [], []
    for folder in sorted(p for p in (ROOT / "plugins").glob("*") if p.is_dir()):
        rel = folder.relative_to(ROOT).as_posix()
        try:
            data = json.loads((folder / "plugin.json").read_bytes())
        except (OSError, json.JSONDecodeError) as e:
            problems.append(f"{rel}: needs a valid plugin.json ({e})")
            continue
        wasm = data.get("wasm", "plugin.wasm")
        found = []
        if data.get("format") != "ambrotype-plugin/1":
            found.append(f'{rel}/plugin.json: "format" must be "ambrotype-plugin/1"')
        if data.get("id") != folder.name:
            found.append(f'{rel}/plugin.json: "id" must be the folder name, "{folder.name}"')
        if not (folder / wasm).is_file():
            found.append(f"{rel}: the built module (plugin.wasm) is missing")
        if not any(folder.rglob("*.rs")):
            found.append(f"{rel}: the source is missing (reviewers rebuild plugin.wasm from it)")
        problems += found
        if found:
            continue
        items.append({
            "id": f"plugin-{folder.name}",
            "kind": "plugin",
            "name": data.get("name", folder.name),
            "author": data.get("author", ""),
            "description": data.get("description", ""),
            "license": "see its source",
            "path": f"{rel}/plugin.json",
            "sha256": hashlib.sha256((folder / "plugin.json").read_bytes()).hexdigest(),
            "files": {wasm: hashlib.sha256((folder / wasm).read_bytes()).hexdigest()},
        })
    return items, problems


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
        print(f"index.json: {len(index['items'])} items")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()

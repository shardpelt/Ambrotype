#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Copy the app's built-in looks and presets from a checkout of the hub repo
(Ambrotypehq/ambrotype-hub) into crates/amb-gui/data/hub/.

    git clone https://github.com/Ambrotypehq/ambrotype-hub ../ambrotype-hub
    python3 tools/sync-hub.py ../ambrotype-hub

The hub is where they are edited. bundle.txt in that folder lists the hub
items the app ships (one id per line); this copies their files, checks each
against the hub's index, and writes an index.json of just those items, plus
SOURCE with the hub commit they came from. Run it, build, and commit the
result.
"""
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys

APP = pathlib.Path(__file__).resolve().parent.parent
DEST = APP / "crates" / "amb-gui" / "data" / "hub"


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    hub = pathlib.Path(sys.argv[1]).resolve()
    index = json.loads((hub / "index.json").read_text())
    by_id = {i["id"]: i for i in index["items"]}
    wanted = [
        line.strip()
        for line in (DEST / "bundle.txt").read_text().splitlines()
        if line.strip() and not line.startswith("#")
    ]
    missing = [w for w in wanted if w not in by_id]
    if missing:
        sys.exit(f"not in the hub: {', '.join(missing)}")
    items = []
    for w in wanted:
        item = by_id[w]
        if item["kind"] not in ("layout", "theme", "preset"):
            sys.exit(f"{w}: only layouts, themes and presets can be built in")
        raw = (hub / item["path"]).read_bytes()
        if hashlib.sha256(raw).hexdigest() != item["sha256"]:
            sys.exit(f"{item['path']}: doesn't match the hub's index (run its tools/build_index.py)")
        items.append(item)
    # Start clean, so items dropped from the bundle go.
    for sub in ("looks", "presets"):
        shutil.rmtree(DEST / sub, ignore_errors=True)
    for item in items:
        out = DEST / item["path"]
        out.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(hub / item["path"], out)
    text = json.dumps({"format": index["format"], "items": items}, indent=2, ensure_ascii=False)
    (DEST / "index.json").write_text(text + "\n")
    commit = subprocess.run(
        ["git", "-C", str(hub), "rev-parse", "HEAD"], capture_output=True, text=True
    ).stdout.strip()
    (DEST / "SOURCE").write_text(f"https://github.com/Ambrotypehq/ambrotype-hub/tree/{commit or 'unknown'}\n")
    print(f"{len(items)} items from {commit[:12] or hub}")


if __name__ == "__main__":
    main()

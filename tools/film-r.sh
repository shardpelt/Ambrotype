#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Download the FILM-R dataset, the dust bench's test scans
# (crates/amb-engine/tests/dust_bench.rs), into DIR. Never commit them.
#
#   tools/film-r.sh <DIR>
#
# FILM-R: 44 damaged 35 mm scans with expert repairs, by Daniela Ivanova,
# John Williamson and Paul Henderson ("Simulating analogue film damage to
# analyse and improve artefact restoration on high-resolution scans",
# Eurographics 2023), scans contributed by analog.cafe. CC BY 4.0:
# https://doi.org/10.6084/m9.figshare.21803304 (about 400 MB).
set -euo pipefail
dir="${1:?usage: film-r.sh <DIR>}"
mkdir -p "$dir"
curl -fsSL https://api.figshare.com/v2/articles/21803304 -o "$dir/figshare.json"
python3 -I -c '
import json, sys
for f in json.load(open(sys.argv[1]))["files"]:
    print(f["name"], f["download_url"])
' "$dir/figshare.json" | while read -r name url; do
  if [ ! -s "$dir/$name" ]; then
    echo "downloading $name"
    curl -fsSL -o "$dir/$name" "$url"
  fi
done
echo "FILM-R is in $dir (CC BY 4.0; credit Ivanova, Williamson and Henderson, Eurographics 2023)"

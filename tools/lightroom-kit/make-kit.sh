#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Build the Lightroom trial kit (docs/lightroom-trial-plan.md): download the
# CC0 raws in raws.tsv, check them, and write one folder per session with
# `amb sweep generate`.
#
#   tools/lightroom-kit/make-kit.sh <KIT-DIR> [SESSION...]
#
# Sessions: 0-check 1a-synthetic 1b-synthetic-detail 2-real 3-real-detail
# 4-lens (all of them when none are named). Put KIT-DIR on a drive formatted
# APFS (Mac) or NTFS (Windows), not exFAT: the raws are then hard links and
# take no extra space. `amb` is found on PATH, in $AMB, or in target/release.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
kit="${1:?usage: make-kit.sh <KIT-DIR> [SESSION...]}"
shift
sessions=("$@")
[ ${#sessions[@]} -eq 0 ] && sessions=(0-check 1a-synthetic 1b-synthetic-detail 2-real 3-real-detail 4-lens)

amb="${AMB:-}"
[ -z "$amb" ] && command -v amb >/dev/null && amb="$(command -v amb)"
for c in "$repo/target/release/amb" "$repo/target/release/amb.exe"; do
  [ -z "$amb" ] && [ -x "$c" ] && amb="$c"
done
[ -n "$amb" ] || { echo "amb not found: build it with cargo build --release -p amb-cli" >&2; exit 1; }

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# The raws, downloaded once and checked.
mkdir -p "$kit/raws"
raws=()
while IFS=$'\t' read -r file sum _camera _what; do
  case "$file" in ''|'#'*) continue ;; esac
  path="$kit/raws/$file"
  if [ ! -f "$path" ] || [ "$(sha256 "$path")" != "$sum" ]; then
    echo "downloading $file"
    curl -fsSL -o "$path" "https://raw.pixls.us/getfile.php/${file%%.*}/nice/$file"
    [ "$(sha256 "$path")" = "$sum" ] || { echo "$file: checksum doesn't match" >&2; exit 1; }
  fi
  raws+=("$path")
done < "$here/raws.tsv"

global=tone,white_balance,presence,tone_curve,hsl,color_grading,calibration,effects
detail=presence,detail,effects
for s in "${sessions[@]}"; do
  out="$kit/$s"
  rm -rf "$out"
  echo "== $s"
  case "$s" in
    0-check) "$amb" sweep generate --out "$out" --groups tone --synthetic ramp "$kit/raws/2414.ARW" ;;
    1a-synthetic) "$amb" sweep generate --out "$out" --groups "$global" --synthetic ramp,hue,colorchecker ;;
    1b-synthetic-detail) "$amb" sweep generate --out "$out" --groups "$detail" --synthetic edges ;;
    2-real) "$amb" sweep generate --out "$out" --groups "$global" "${raws[@]}" ;;
    3-real-detail) "$amb" sweep generate --out "$out" --groups "$detail" "$kit/raws/3581.NEF" "$kit/raws/983.CR2" ;;
    4-lens) "$amb" sweep generate --out "$out" --groups lens "${raws[@]}" ;;
    *) echo "unknown session $s" >&2; exit 1 ;;
  esac
done
echo "kit ready in $kit (raws are CC0, from raw.pixls.us)"

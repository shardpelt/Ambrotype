#!/bin/bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Claude Code cloud sessions: install the system libraries the editor
# (amb-gui, Tauri 2) needs to build and test, and to try it without a screen
# (Xvfb, xdotool, ImageMagick's import). The other crates only need
# Rust, which the cloud image has. Does nothing on your own computer.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

packages=(pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev librsvg2-dev libxdo-dev xvfb xdotool imagemagick)
missing=()
for p in "${packages[@]}"; do
  dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p")
done
if [ ${#missing[@]} -eq 0 ]; then
  exit 0
fi

export DEBIAN_FRONTEND=noninteractive
sudo_cmd=""
[ "$(id -u)" -ne 0 ] && sudo_cmd="sudo"
$sudo_cmd apt-get update -qq
$sudo_cmd apt-get install -y -qq --no-install-recommends "${missing[@]}" >/dev/null
echo "Installed for the editor: ${missing[*]}"

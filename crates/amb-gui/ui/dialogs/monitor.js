// SPDX-License-Identifier: GPL-3.0-or-later
// Monitor colour (Settings): the preview in the monitor's own colours, from
// the desktop's profile or a file (src/monitor.rs). On macOS and Windows the
// system does this itself, and Settings just says so.
"use strict";

function showMonitorStatus(s) {
  const sel = $("settings-monitor");
  sel.value = s.choice.mode;
  sel.disabled = s.by_system;
  $("settings-monitor-pick").hidden = s.by_system || s.choice.mode !== "file";
  $("settings-monitor-note").textContent = s.note;
}

async function chooseMonitor(choice) {
  try {
    const s = await invoke("set_monitor_profile", { choice });
    showMonitorStatus(s);
    requestPreview(false);
  } catch (e) {
    toast(String(e), true);
  }
}

async function pickMonitorProfile() {
  const path = await invoke("pick_monitor_profile");
  if (path) await chooseMonitor({ mode: "file", path });
  else showMonitorStatus(await invoke("monitor_profile"));
}

function wireMonitor() {
  invoke("monitor_profile").then((s) => {
    showMonitorStatus(s);
    // The first preview may have come before the profile was read.
    if (s.profile) requestPreview(false);
  }).catch(() => { /* stays sRGB */ });
  // The window went to a monitor with another profile.
  tauri.event.listen("monitor-changed", (e) => {
    showMonitorStatus(e.payload);
    requestPreview(false);
  });
  $("settings-monitor").addEventListener("change", (e) => {
    if (e.target.value === "file") pickMonitorProfile();
    else chooseMonitor({ mode: e.target.value });
  });
  $("settings-monitor-pick").addEventListener("click", pickMonitorProfile);
}

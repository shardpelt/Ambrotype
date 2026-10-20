// SPDX-License-Identifier: GPL-3.0-or-later
// Settings → General → Interface: the built-in interface, or a custom one
// (a folder with an index.html; src/interface.rs) shown in its place. A
// custom one is code with the app's full reach, so choosing it asks first.
"use strict";

async function renderInterfaces() {
  const sel = $("settings-interface");
  try {
    const info = await invoke("interfaces");
    const name = (i) => i.name ?? i.path.split(/[\\/]/).pop();
    const opts = [{ value: "", text: "Built-in" }, ...info.installed.map((i) => ({ value: i.path, text: name(i) }))];
    if (info.active && !info.installed.some((i) => i.path === info.active)) opts.push({ value: info.active, text: info.active });
    sel.replaceChildren(...opts.map((o) => Object.assign(document.createElement("option"), { value: o.value, textContent: o.text })));
    sel.value = info.active ?? "";
    $("settings-interface-share").disabled = !info.active;
  } catch (e) {
    toast(String(e), true);
  }
}

async function useInterface(path) {
  if (path && !await askYes(
    "Show this interface instead of the built-in one?\n\n"
    + "It's a program with the app's full reach (your photos, edits and settings): use your own, or one you have read.\n\n"
    + "To come back: its own way back, the menu bar's Interface → Back to the Built-in Interface, or start Ambrotype with --builtin-ui.",
  )) {
    renderInterfaces();
    return;
  }
  try {
    await invoke("set_interface", { path: path || null });
  } catch (e) {
    toast(String(e), true);
    renderInterfaces();
  }
}

// Offer the chosen interface to the hub: show what would be sent, then send
// it with the GitHub CLI or in the browser, as plugins do.
async function shareInterface() {
  const path = $("settings-interface").value;
  if (!path) return;
  try {
    const plan = await invoke("interface_share_plan", { path });
    const how = plan.gh_user
      ? `as a pull request from @${plan.gh_user} (the GitHub CLI on this computer)`
      : "through GitHub's upload page in your browser (drag the files from the folder that opens, then “Propose changes”)";
    const list = plan.files.slice(0, 12).map((f) => `  ${f.path}`).join("\n") + (plan.files.length > 12 ? `\n  … ${plan.files.length - 12} more` : "");
    if (!await askYes(`Send ${plan.files.length} files to ${plan.target} in ${plan.repo}, ${how}?\n\n${list}\n\nThe hub's reviewers read it before anyone can install it.`)) return;
    const url = await invoke("share_interface", { path, browser: !plan.gh_user });
    toast(plan.gh_user ? `Pull request: ${url}` : "GitHub's upload page is open in your browser");
  } catch (e) {
    toast(String(e), true);
  }
}

function wireInterfaces() {
  $("settings-interface-share").addEventListener("click", shareInterface);
  $("settings-interface").addEventListener("change", (e) => useInterface(e.target.value));
  $("settings-interface-pick").addEventListener("click", async () => {
    const path = await invoke("pick_interface_folder");
    if (path) useInterface(path);
  });
  $("settings-interface-folder").addEventListener("click", () => invoke("open_interfaces_folder").catch((e) => toast(String(e), true)));
}

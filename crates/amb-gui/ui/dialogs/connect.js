// SPDX-License-Identifier: GPL-3.0-or-later
// Connect an AI app (Help menu, Ctrl+K): any app that runs MCP servers
// gets Ambrotype's tools from `amb mcp`, on the user's own AI plan; any
// command-line agent can run `amb` in a terminal. The apps listed are data
// (ai-apps.json), not a choice of maker. Rust side: src/commands/connect.rs.
"use strict";

const connect = { apps: null, amb: null };

// A command-line word for a path: as it is, or quoted for the system's shell.
function shellWord(p) {
  if (!/[\s"'$`&()]/.test(p)) return p;
  return /^[A-Za-z]:\\/.test(p) ? `"${p}"` : `'${p.replace(/'/g, "'\\''")}'`;
}

function connectSnippet(app) {
  const amb = connect.amb;
  if (app.snippet === "json") return JSON.stringify({ mcpServers: { ambrotype: { command: amb, args: ["mcp"] } } }, null, 2);
  if (app.snippet === "vscode") return JSON.stringify({ servers: { ambrotype: { type: "stdio", command: amb, args: ["mcp"] } } }, null, 2);
  return app.snippet.replace("{amb}", shellWord(amb));
}

function renderConnect() {
  const app = connect.apps[Number($("connect-app").value)] ?? connect.apps[0];
  $("connect-where").textContent = app.where;
  $("connect-snippet").textContent = connect.amb ? connectSnippet(app) : "";
}

async function copyText(text, what) {
  try {
    await navigator.clipboard.writeText(text);
    toast(`${what} copied`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function connectFlow() {
  if (!connect.apps) connect.apps = (await (await fetch("ai-apps.json")).json()).apps;
  const info = await invoke("ai_connect_info");
  connect.amb = info.amb;
  $("connect-command").value = info.amb ? `${shellWord(info.amb)} mcp` : "";
  $("connect-missing").hidden = Boolean(info.amb);
  for (const id of ["connect-copy-command", "connect-copy"]) $(id).disabled = !info.amb;
  $("connect-folder").textContent = info.folder;
  $("connect-app").replaceChildren(...connect.apps.map((a, i) => Object.assign(document.createElement("option"), { value: i, textContent: a.name })));
  renderConnect();
  $("connect-dialog").showModal();
}

function wireConnect() {
  $("connect-app").addEventListener("change", renderConnect);
  $("connect-copy-command").addEventListener("click", () => copyText($("connect-command").value, "The command"));
  $("connect-copy").addEventListener("click", () => copyText($("connect-snippet").textContent, "The setup"));
  $("assistant-connect").addEventListener("click", () => connectFlow().catch((e) => toast(String(e), true)));
  $("connect-terminal").addEventListener("click", () => invoke("open_terminal").catch((e) => toast(String(e), true)));
}

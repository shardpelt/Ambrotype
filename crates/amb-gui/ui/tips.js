// SPDX-License-Identifier: GPL-3.0-or-later
// Tips: the app teaching by showing (AGENTS.md). Now and then, a feature
// from the features list (docs/FEATURES.md, `ui.features`) with a button
// to try it:
// - while something takes a moment (the busy spinner is up): a small card
//   over the photo, at most once every 90 seconds;
// - where text normally is: the search box and the preset filter suggest
//   something to type.
// Features whose commands you have already used are skipped. Two switches
// in Settings → General, both on: "Tips while waiting" and "Tips in menus
// and empty boxes". The card's Don't show tips turns both off, and so does
// Not now three times in a row. Never during UI tests (unless one asks).
"use strict";

const tips = { last: 0, timer: 0, hideTimer: 0, used: new Set() };

function tipsOn(kind) {
  return store.text(kind === "wait" ? "amb.tipsWait" : "amb.tipsBoxes") !== "0";
}

function setTipsOn(kind, on) {
  store.setText(kind === "wait" ? "amb.tipsWait" : "amb.tipsBoxes", on ? "1" : "0");
}

function markUsed(key) {
  if (tips.used.has(key)) return;
  tips.used.add(key);
  store.set("amb.used", [...tips.used].slice(-400));
}

// A feature to show, with what its Try button does: one whose commands
// haven't been used, that can be used from here.
function pickTip() {
  const list = (ui.features ?? []).map((f) => {
    const target = f.names.map((n) => howTarget(n)).find(Boolean);
    return target ? { f, target } : null;
  }).filter((x) => x && !tips.used.has(x.target.key));
  if (!list.length) return null;
  return list[Math.floor(Math.random() * list.length)];
}

// The card: "Tip: <what you'd want>", where it is, and Try it.
function showTip(tip = pickTip()) {
  if (!tip) return false;
  const card = $("tip-card");
  $("tip-want").textContent = tip.f.want;
  const how = tip.f.how.replace(/`[^`]*`\.?/g, "").replace(/\s+\./g, ".").trim();
  $("tip-how").textContent = how.charAt(0).toUpperCase() + how.slice(1);
  $("tip-try").onclick = () => {
    hideTip();
    setDismissed(0);
    markUsed(tip.target.key);
    requestAnimationFrame(() => tip.target.run());
  };
  card.hidden = false;
  tips.last = Date.now();
  clearTimeout(tips.hideTimer);
  tips.hideTimer = setTimeout(hideTip, 12000);
  return true;
}

// How many tips in a row were put away with Not now.
function dismissed() {
  return Number(store.text("amb.tipsDismissed") ?? 0) || 0;
}

function setDismissed(n) {
  store.setText("amb.tipsDismissed", String(n));
}

// No more tips, either kind; Settings → General shows them off.
function stopTips(why) {
  hideTip();
  setDismissed(0);
  for (const [id, kind] of [["settings-tips-wait", "wait"], ["settings-tips-boxes", "boxes"]]) {
    setTipsOn(kind, false);
    $(id).checked = false;
  }
  toast(`${why} Settings → General turns them on again.`);
}

function tipNotNow() {
  const n = dismissed() + 1;
  if (n >= 3) return stopTips("No more tips.");
  setDismissed(n);
  hideTip();
}

function hideTip() {
  clearTimeout(tips.hideTimer);
  $("tip-card").hidden = true;
}

// Waiting: the spinner up for a moment, a tip if it's been a while.
function busyChanged() {
  clearTimeout(tips.timer);
  if ($("busy").hidden || !tipsOn("wait") || window.__ambUiTest) return;
  tips.timer = setTimeout(async () => {
    if ($("busy").hidden || Date.now() - tips.last < 90000) return;
    await loadFeatures();
    showTip();
  }, 700);
}

// Empty boxes: now and then, a suggestion where the hint text is (the
// search box asks for it each time it opens).
function boxTip(input, plain) {
  if (!tipsOn("boxes") || Math.random() > 0.4 || !ui.features?.length) {
    input.placeholder = plain;
    return;
  }
  const tip = pickTip();
  input.placeholder = tip ? `${plain}… try “${tip.f.want.replace(/\s*\(.*\)$/, "").toLowerCase()}”` : plain;
}

function wireTips() {
  try {
    for (const k of JSON.parse(localStorage.getItem("amb.used") ?? "[]")) tips.used.add(k);
  } catch {
    // Nothing kept yet.
  }
  // Commands run count as used, however they were started.
  for (const a of KEY_ACTIONS) {
    const run = a.run;
    a.run = (...args) => {
      markUsed(`a:${a.id}`);
      return run(...args);
    };
  }
  new MutationObserver(busyChanged).observe($("busy"), { attributes: true, attributeFilter: ["hidden"] });
  $("tip-close").addEventListener("click", tipNotNow);
  $("tip-never").addEventListener("click", () => stopTips("Tips are off."));
  for (const [id, kind] of [["settings-tips-wait", "wait"], ["settings-tips-boxes", "boxes"]]) {
    const box = $(id);
    box.checked = tipsOn(kind);
    box.addEventListener("change", () => {
      setTipsOn(kind, box.checked);
      if (box.checked) setDismissed(0);
    });
  }
  // The preset filter suggests a group or a preset to type.
  $("preset-filter").addEventListener("focus", () => {
    const f = $("preset-filter");
    const names = [...new Set((ui.presets ?? []).flatMap((p) => [p.group, p.name]).filter(Boolean))];
    f.placeholder = !f.value && tipsOn("boxes") && names.length && Math.random() < 0.5
      ? `Filter presets… try “${names[Math.floor(Math.random() * names.length)].toLowerCase()}”`
      : "Filter presets";
  });
  loadFeatures();
}

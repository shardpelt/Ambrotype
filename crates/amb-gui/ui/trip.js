// SPDX-License-Identifier: GPL-3.0-or-later
// The Trip block (the Trippy task): big buttons that put playful effects on
// the photo in one click: rainbow foil (Holographic), psychedelic colour
// (Hue trip), a light leak, pop duotone, or Trip me, a new random mix each
// click. They are ordinary effects (amb:Effects), so the Effects panel
// tunes them and Undo takes each click back.
"use strict";

// The plugins a trip is made of, and the recipes shown as buttons.
const TRIP_PLUGINS = ["hue-trip", "holographic", "light-leak", "duotone"];
const TRIP_BUTTONS = [
  ["hue-trip", "Acid"], ["hue-trip", "Pop art"], ["hue-trip", "Dream"],
  ["holographic", "Foil sticker"], ["holographic", "Oil slick"], ["holographic", "Prism"],
  ["duotone", "Pop pink"],
];

function tripEffects() {
  return effectsOf(currentValue("amb:Effects"));
}

function tripSeed() {
  return Math.floor(Math.random() * 2 ** 31);
}

// Put `effects` on the photo in place of the trip's own (other effects stay).
async function setTrip(effects, what) {
  if (!ui.info) return toast("Open a photo first.", true);
  const kept = tripEffects().filter((e) => !TRIP_PLUGINS.includes(e.plugin));
  const list = [...kept, ...effects];
  await run(list.length
    ? [{ cmd: "set_param", name: "amb:Effects", value: JSON.stringify(list) }]
    : [{ cmd: "reset_param", name: "amb:Effects" }], what);
}

function tripRecipe(plugin, name) {
  const m = fx.plugins.find((p) => p.id === plugin)?.manifest;
  return m?.recipes?.find((r) => r.name === name)?.params ?? {};
}

// One recipe: replaces that plugin's effect, keeps the rest of the trip.
async function tripApply(plugin, name) {
  const others = tripEffects().filter((e) => TRIP_PLUGINS.includes(e.plugin) && e.plugin !== plugin);
  await setTrip([...others, { plugin, params: tripRecipe(plugin, name), amount: 100, seed: tripSeed() }], `Trip: ${name}`);
}

// A random number in [lo, hi], rounded to `step`.
function tripRoll(lo, hi, step = 1) {
  return Math.round((lo + Math.random() * (hi - lo)) / step) * step;
}

// Trip me: psychedelic colour or foil (or both), now and then a light leak.
function tripMix() {
  const out = [];
  const pick = Math.random();
  if (pick < 0.7) {
    out.push({ plugin: "hue-trip", params: { turn: tripRoll(0, 360), cycles: tripRoll(0.3, 3, 0.1), colour: tripRoll(120, 260), bands: Math.random() < 0.25 ? tripRoll(3, 6) : 0, paint: tripRoll(20, 70) } });
  }
  if (pick > 0.4) {
    out.push({ plugin: "holographic", params: { strength: tripRoll(35, 80), bands: tripRoll(2, 16, 0.5), angle: tripRoll(0, 180), follow: tripRoll(10, 95), shift: tripRoll(0, 360), pastel: tripRoll(0, 50) } });
  }
  if (Math.random() < 0.3) {
    out.push({ plugin: "light-leak", params: { strength: tripRoll(40, 80), size: tripRoll(25, 60), edge: ["Left", "Right", "Top", "Bottom"][tripRoll(0, 3)], colour: ["Amber", "Red", "Pink", "Gold"][tripRoll(0, 3)], variation: tripRoll(0, 99) } });
  }
  return out.map((e) => ({ ...e, amount: 100, seed: tripSeed() }));
}

function wireTrip() {
  const box = $("trip-buttons");
  const big = Object.assign(document.createElement("button"), { type: "button", className: "trip-me", textContent: "Trip me", title: "A new random mix of psychedelic colour, rainbow foil and light leaks, each click (Undo takes it back)" });
  big.addEventListener("click", () => setTrip(tripMix(), "Trip me"));
  const buttons = TRIP_BUTTONS.map(([plugin, name]) => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: "trip-recipe", textContent: name });
    b.dataset.plugin = plugin;
    b.addEventListener("click", () => tripApply(plugin, name));
    return b;
  });
  const clear = Object.assign(document.createElement("button"), { type: "button", className: "small", textContent: "Back to normal", title: "Take the trip's effects off the photo (other effects stay)" });
  clear.addEventListener("click", () => setTrip([], "Trip: back to normal"));
  box.replaceChildren(big, ...buttons, clear);
  // Each recipe's tooltip is its plugin's description, once they're loaded.
  loadEffectPlugins().then(() => {
    for (const b of buttons) b.title = fx.plugins.find((p) => p.id === b.dataset.plugin)?.manifest?.description ?? "";
  });
}

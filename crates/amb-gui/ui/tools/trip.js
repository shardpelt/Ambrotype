// SPDX-License-Identifier: GPL-3.0-or-later
// Trip me and Back to normal, the Trippy task's commands (the task itself
// is a task plugin, plugins/trippy): a new random mix of psychedelic
// colour, rainbow foil, heat-camera colour, light leaks and kaleidoscopes
// each click, or the trip taken off.
// They are ordinary effects (amb:Effects), so the Effects panel tunes them
// and Undo takes each click back. Also task tools' effect buttons
// (`setEffect`), for any task plugin.
"use strict";

// The plugins a trip is made of.
const TRIP_PLUGINS = ["hue-trip", "holographic", "light-leak", "duotone", "paint", "kaleidoscope", "heat-camera"];

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

// An effect plugin on the photo with `params` (or a recipe's), `where`
// (only the highlights, the blues…) and `amount` (to 200), in place of the
// one of that plugin already there; one undo step.
async function setEffect(plugin, opts = {}, what = null) {
  if (!ui.info) return toast("Open a photo first.", true);
  const commands = await effectCommands(plugin, opts);
  if (!commands) return toast(`The effect ${plugin} isn't installed.`, true);
  await run(commands, what ?? (opts.recipe || plugin));
}

// The commands setEffect runs (null when the plugin isn't installed). A
// `seed` keeps a hover preview and the click that follows it the same.
async function effectCommands(plugin, { recipe = "", params = {}, where = null, amount = 100, seed = tripSeed() } = {}) {
  if (!fx.plugins.length) await loadEffectPlugins();
  if (!fx.plugins.some((p) => p.id === plugin)) return null;
  const values = { ...(recipe ? tripRecipe(plugin, recipe) : {}), ...params };
  const entry = { plugin, params: values, amount: amount ?? 100, seed, ...(where ? { where } : {}) };
  const list = [...tripEffects().filter((e) => e.plugin !== plugin), entry];
  return [{ cmd: "set_param", name: "amb:Effects", value: JSON.stringify(list) }];
}

// A random number in [lo, hi], rounded to `step`.
function tripRoll(lo, hi, step = 1) {
  return Math.round((lo + Math.random() * (hi - lo)) / step) * step;
}

// Trip me: psychedelic colour or foil (or both), or now and then heat-camera
// colour; sometimes a light leak, sometimes a kaleidoscope.
function tripMix() {
  const out = [];
  const pick = Math.random();
  if (Math.random() < 0.12) {
    out.push({ plugin: "heat-camera", params: { palette: ["Ironbow", "Rainbow", "Arctic"][tripRoll(0, 2)], contrast: tripRoll(80, 150), shift: tripRoll(-15, 15), cycles: Math.random() < 0.4 ? tripRoll(2, 5, 0.5) : 1 } });
  } else if (pick < 0.7) {
    out.push({ plugin: "hue-trip", params: { turn: tripRoll(0, 360), cycles: tripRoll(0.3, 3, 0.1), colour: tripRoll(120, 260), bands: Math.random() < 0.25 ? tripRoll(3, 6) : 0, paint: tripRoll(20, 70) } });
  }
  if (pick > 0.4) {
    out.push({ plugin: "holographic", params: { strength: tripRoll(35, 80), bands: tripRoll(2, 16, 0.5), angle: tripRoll(0, 180), follow: tripRoll(10, 95), shift: tripRoll(0, 360), pastel: tripRoll(0, 50) } });
  }
  if (Math.random() < 0.3) {
    out.push({ plugin: "light-leak", params: { strength: tripRoll(40, 80), size: tripRoll(25, 60), edge: ["Left", "Right", "Top", "Bottom"][tripRoll(0, 3)], colour: ["Amber", "Red", "Pink", "Gold"][tripRoll(0, 3)], variation: tripRoll(0, 99) } });
  }
  if (Math.random() < 0.2) {
    const fractal = Math.random() < 0.4;
    out.push({ plugin: "kaleidoscope", params: { style: fractal ? "Fractal" : "Mirrors", slices: tripRoll(3, 12), turn: tripRoll(0, 360), zoom: tripRoll(70, 160), x: 50, y: 50, depth: tripRoll(3, 7), twist: tripRoll(0, 60) } });
  }
  return out.map((e) => ({ ...e, amount: 100, seed: tripSeed() }));
}

const tripMe = () => setTrip(tripMix(), "Trip me");
const tripClear = () => setTrip([], "Trip: back to normal");

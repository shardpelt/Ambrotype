// SPDX-License-Identifier: GPL-3.0-or-later
// Trip me and Back to normal, the Trippy task's commands (the task itself
// is a task plugin, plugins/trippy): a new random mix of psychedelic
// colour, rainbow foil and light leaks each click, or the trip taken off.
// They are ordinary effects (amb:Effects), so the Effects panel tunes them
// and Undo takes each click back. Also task tools' effect buttons
// (`setEffect`), for any task plugin.
"use strict";

// The plugins a trip is made of.
const TRIP_PLUGINS = ["hue-trip", "holographic", "light-leak", "duotone", "paint"];

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

// An effect plugin on the photo with `params` (or a recipe's), in place of
// the one of that plugin already there; one undo step.
async function setEffect(plugin, { recipe = "", params = {} } = {}, what = null) {
  if (!ui.info) return toast("Open a photo first.", true);
  if (!fx.plugins.length) await loadEffectPlugins();
  if (!fx.plugins.some((p) => p.id === plugin)) return toast(`The effect ${plugin} isn't installed.`, true);
  const values = { ...(recipe ? tripRecipe(plugin, recipe) : {}), ...params };
  const list = [...tripEffects().filter((e) => e.plugin !== plugin), { plugin, params: values, amount: 100, seed: tripSeed() }];
  await run([{ cmd: "set_param", name: "amb:Effects", value: JSON.stringify(list) }], what ?? (recipe || plugin));
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

const tripMe = () => setTrip(tripMix(), "Trip me");
const tripClear = () => setTrip([], "Trip: back to normal");

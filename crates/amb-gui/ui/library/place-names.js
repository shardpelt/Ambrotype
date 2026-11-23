// SPDX-License-Identifier: GPL-3.0-or-later
// Add place names (the Map view's bar, the "place-names" action; `amb place
// --names`): the city, state and country of the selected photos' places
// (all shown when none is selected), looked up on OpenStreetMap only when
// asked and kept in the sidecars, where search finds them. The lookup is
// amb_library::places.
"use strict";

const placeNames = { busy: false };

// "Paris, France" for a photo whose sidecar has names, else "".
function placeNamesText(p) {
  return [p.city, p.country].filter(Boolean).join(", ");
}

async function placeNamesFlow() {
  if (placeNames.busy) return;
  const chosen = selectedPhotos();
  const targets = (chosen.length ? chosen : lib.shown).filter((p) => placeOf(p));
  if (!targets.length) {
    return toast(t("None of these photos has a place. Place one first (drag it onto the map, or a GPX track)."), true);
  }
  const ask = t("Look up town and country names for {photos} on OpenStreetMap?", { photos: plural(targets.length, "photo") });
  if (!(await askYes(ask))) return;
  placeNames.busy = true;
  const button = document.querySelector(".map-bar [data-place-names]");
  if (button) button.disabled = true;
  try {
    const r = await invoke("library_place_names", { paths: targets.map((p) => p.path), replace: false });
    for (const u of r.updates) {
      photoChanged(u.path, { city: u.info.city ?? null, state: u.info.state ?? null, country: u.info.country ?? null });
    }
    const failed = r.failed.length ? ` · ${t("{n} failed: {why}", { n: r.failed.length, why: r.failed[0][1] })}` : "";
    toast(t("Named {n} places", { n: r.named }) + failed, !r.named && r.failed.length > 0);
  } catch (e) {
    showError(e);
  } finally {
    placeNames.busy = false;
    if (button) button.disabled = false;
  }
}

function wirePlaceNames() {
  tauri.event.listen("place-names-progress", (e) => {
    const [done, total] = e.payload;
    if (placeNames.busy && total) status(t("Looking up place names… {done} of {total}", { done, total }));
  });
}

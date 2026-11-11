// SPDX-License-Identifier: GPL-3.0-or-later
// Guess places (the Map view's bar; `amb place --guess`): photos without a
// place get one from the photos that have one, taken around the same time
// (a camera without GPS beside a phone that has it) or looking the same.
// Each guess is listed with why, ticked; nothing is written until you
// confirm. The guessing itself is amb_library::places.
"use strict";

async function guessPlacesFlow() {
  const targets = selectedPhotos().length > 1 ? selectedPhotos() : lib.shown;
  if (!targets.some((p) => p.gps || p.camera_gps)) {
    return toast("None of these photos has a place to guess from. Place one first (drag it onto the map, or a GPX track).", true);
  }
  const dlg = $("guess-dialog");
  const list = $("guess-list");
  let guesses = [];
  const roll = async () => {
    $("guess-count").textContent = "Guessing…";
    $("guess-ok").disabled = true;
    try {
      guesses = await invoke("library_guess_places", {
        paths: targets.map((p) => p.path),
        within: Number($("guess-within").value) || 30,
        offset: (Number($("guess-clock").value) || 0) * 60,
      });
    } catch (e) {
      $("guess-count").textContent = String(e);
      return;
    }
    const without = targets.filter((p) => !(p.gps || p.camera_gps)).length;
    $("guess-count").textContent = guesses.length
      ? `${guesses.length} of the ${without} without a place have a guess.`
      : `No guesses for the ${without} without a place: none was taken near a placed one (try more minutes, or the clock), or looks the same.`;
    list.replaceChildren(...guesses.map((g, i) => {
      const p = libPhoto(g.path);
      const row = Object.assign(document.createElement("label"), { className: "guess-row" });
      const box = Object.assign(document.createElement("input"), { type: "checkbox", checked: true });
      box.dataset.i = String(i);
      const img = Object.assign(document.createElement("img"), { alt: "", loading: "lazy" });
      const url = p && cachedThumb(p);
      if (url) img.src = url;
      const text = Object.assign(document.createElement("span"), { className: "guess-text" });
      text.append(Object.assign(document.createElement("b"), { textContent: p?.name ?? fileName(g.path) }), document.createElement("br"),
        `${g.lat.toFixed(5)}, ${g.lon.toFixed(5)} · ${g.by === "look" ? "by look" : "by time"}: ${g.why}`);
      row.append(box, img, text);
      return row;
    }));
    $("guess-ok").disabled = !guesses.length;
  };
  $("guess-within").onchange = roll;
  $("guess-clock").onchange = roll;
  $("guess-all").onclick = () => list.querySelectorAll("input").forEach((b) => { b.checked = true; });
  dlg.showModal();
  await roll();
  const answer = await new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue), { once: true }));
  // Its boxes keep the focus once closed, and would take the keys.
  if (dlg.contains(document.activeElement)) document.activeElement.blur();
  if (answer !== "ok") return;
  const chosen = [...list.querySelectorAll("input:checked")].map((b) => guesses[Number(b.dataset.i)]);
  if (!chosen.length) return;
  try {
    // One write per photo: each has its own place.
    for (const g of chosen) takePlaces(await setInfo([g.path], { gps: `${g.lat.toFixed(6)},${g.lon.toFixed(6)}` }));
    toast(`Placed ${chosen.length} photo${chosen.length === 1 ? "" : "s"} by guess`);
    browse.map.fitted = false;
    if (browseOn()) renderBrowse();
  } catch (e) {
    toast(String(e), true);
  }
}

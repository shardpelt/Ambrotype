// SPDX-License-Identifier: GPL-3.0-or-later
// A place for each photo, typed or pasted: in the library's panel (for the
// selection) and the viewer's info panel. It takes "52.3731, 4.8922",
// degrees and minutes ("52°22'23"N 4°53'32"E"), or a link copied from
// Google Maps, OpenStreetMap or Apple Maps. Map shows it on the Map view;
// × takes it out (a camera's own GPS stays). Written where Lightroom keeps
// it (exif:GPSLatitude/Longitude in the sidecar).
"use strict";

// "lat,lon" (six decimals) from what was typed or pasted, or null.
function parsePlace(text) {
  const t = String(text ?? "").trim();
  if (!t) return null;
  const ok = (lat, lon) => Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180
    ? `${lat.toFixed(6)},${lon.toFixed(6)}` : null;
  // Links: Google (@lat,lon or !3dlat!4dlon or q=lat,lon), Apple (ll= / q=),
  // OpenStreetMap (mlat=&mlon= or #map=zoom/lat/lon).
  const link = [
    /!3d(-?\d+(?:\.\d+)?)!4d(-?\d+(?:\.\d+)?)/,
    /@(-?\d+(?:\.\d+)?),(-?\d+(?:\.\d+)?)/,
    /[?&](?:q|ll|query|sll|daddr)=(-?\d+(?:\.\d+)?)(?:,|%2C)\s*(-?\d+(?:\.\d+)?)/i,
    /mlat=(-?\d+(?:\.\d+)?)&mlon=(-?\d+(?:\.\d+)?)/,
    /#map=\d+\/(-?\d+(?:\.\d+)?)\/(-?\d+(?:\.\d+)?)/,
  ];
  if (/^[a-z]+:\/\//i.test(t) || t.includes("maps")) {
    for (const re of link) {
      const m = t.match(re);
      if (m) return ok(Number(m[1]), Number(m[2]));
    }
    return null;
  }
  // Degrees, minutes, seconds with N S E W.
  const dms = [...t.matchAll(/(\d+(?:[.,]\d+)?)\s*°\s*(?:(\d+(?:[.,]\d+)?)\s*['′’]\s*)?(?:(\d+(?:[.,]\d+)?)\s*(?:["″”]|''))?\s*([NSEWnsew])/g)];
  if (dms.length === 2) {
    const deg = (m) => {
      const n = (s) => Number((s ?? "0").replace(",", "."));
      const v = n(m[1]) + n(m[2]) / 60 + n(m[3]) / 3600;
      return /[SWsw]/.test(m[4]) ? -v : v;
    };
    const [a, b] = dms;
    const latFirst = /[NSns]/.test(a[4]);
    return latFirst ? ok(deg(a), deg(b)) : ok(deg(b), deg(a));
  }
  // Two decimal numbers.
  const nums = t.replace(/[−–]/g, "-").match(/-?\d+(?:\.\d+)?/g);
  if (nums?.length === 2) return ok(Number(nums[0]), Number(nums[1]));
  return null;
}

// The Map view, centred on `gps` ("lat,lon"), closer in.
function showPlaceOnMap(gps) {
  const [lat, lon] = gps.split(",").map(Number);
  if (!libraryOn()) return;
  const m = browse.map;
  m.z = Math.max(m.z, 13);
  [m.x, m.y] = project(lat, lon, m.z);
  m.fitted = true;
  setLibraryView("map");
}

// The library panel's Place row, for the selection.
function renderPlaceField(sel) {
  const input = $("lib-place");
  if (!input) return;
  const n = sel.length;
  const set = new Set(sel.map((p) => (p.gps ? p.gps.map((v) => v.toFixed(6)).join(",") : "")));
  const one = set.size === 1 ? [...set][0] : undefined;
  if (document.activeElement !== input) input.value = one ?? "";
  const cam = n === 1 && !sel[0].gps && sel[0].camera_gps;
  input.placeholder = n > 1 && one === undefined ? "(mixed)" : cam ? `Camera: ${cam.map((v) => v.toFixed(5)).join(", ")}` : "52.3731, 4.8922 or a map link";
  input.disabled = !n;
  $("lib-place-pick").disabled = !n;
  $("lib-place-map").disabled = !(one || cam);
  $("lib-place-clear").disabled = !sel.some((p) => p.gps);
  input.classList.remove("bad");
}

function wirePlaceField() {
  const input = $("lib-place");
  const apply = () => {
    const sel = selectedPhotos();
    if (!sel.length) return;
    const text = input.value.trim();
    if (!text) return;
    const gps = parsePlace(text);
    if (!gps) {
      input.classList.add("bad");
      return toast("Not a place: type latitude, longitude (52.3731, 4.8922), or paste a map link.", true);
    }
    input.classList.remove("bad");
    placePhotos(sel.map((p) => p.path), gps).then(() => renderPanel());
  };
  input.addEventListener("change", apply);
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") input.blur();
  });
  // A pasted link is taken at once.
  input.addEventListener("paste", () => setTimeout(() => parsePlace(input.value) && input.blur(), 0));
  $("lib-place-pick").addEventListener("click", async () => {
    const sel = selectedPhotos();
    if (!sel.length) return;
    const at = sel.map((p) => p.gps ?? p.camera_gps).find(Boolean) ?? null;
    const gps = await pickPlace(at, sel.length > 1 ? `Where the ${sel.length} photos were taken` : `Where ${sel[0].name} was taken`);
    if (gps) placePhotos(sel.map((p) => p.path), gps).then(() => renderPanel());
  });
  $("lib-place-clear").addEventListener("click", () => placePhotos(selectedPhotos().map((p) => p.path), "").then(() => renderPanel()));
  $("lib-place-map").addEventListener("click", () => {
    const p = selectedPhotos()[0];
    const at = p && (p.gps ?? p.camera_gps);
    if (at) showPlaceOnMap(at.map((v) => v.toFixed(6)).join(","));
  });
}

// The viewer's info panel: a Place box for the photo on screen.
function viewerPlaceRow(d, el) {
  const at = d.gps ?? null;
  const cam = !at && d.camera_gps;
  const input = el("input", { type: "text", spellcheck: false, value: at ? at.map((v) => v.toFixed(6)).join(", ") : "", placeholder: cam ? `Camera: ${cam.map((v) => v.toFixed(5)).join(", ")}` : "52.3731, 4.8922 or a map link" });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const gps = input.value.trim() ? parsePlace(input.value) : "";
      if (gps === null) return toast("Not a place: type latitude, longitude, or paste a map link.", true);
      viewerChange({ gps });
    }
  });
  const pick = el("button", { type: "button", className: "small", textContent: "Pick…", title: "Choose it on a map, or find an address or place by name" });
  pick.addEventListener("click", async () => {
    const gps = await pickPlace(at ?? cam ?? null, `Where ${d.name ?? fileName(d.path)} was taken`);
    if (gps) viewerChange({ gps });
    slides.el?.focus();
  });
  return el("div", { className: "ss-place" }, input, pick);
}

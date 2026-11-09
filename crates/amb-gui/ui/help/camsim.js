// SPDX-License-Identifier: GPL-3.0-or-later
// Camera simulator (Help → Camera simulator): a simple scene drawn as a
// camera would see it, so shutter speed, aperture, ISO and focal length
// can be tried and their effects seen at once. Nothing here edits a photo.
//
// The scene is layers at distances (sky, hills, trees, a person, a ball
// rolling past, grass up close), placed with a pinhole camera on a 36 mm
// wide sensor. From the settings:
// - exposure: the scene's light (EV at ISO 100) against the camera's EV,
//   log2(N² / t) − log2(ISO / 100); each stop off doubles or halves it;
// - depth of field: each layer's blur circle f² / (N (s − f)) · |D − s| / D
//   on the sensor, for focus distance s;
// - motion: the ball's movement during the exposure, and camera shake
//   when the shutter is slower than 1 / focal length;
// - noise: grain that grows with ISO.
"use strict";

const CAM = {
  W: 640,
  H: 400,
  SENSOR: 36, // mm
  SHUTTERS: [1 / 4000, 1 / 2000, 1 / 1000, 1 / 500, 1 / 250, 1 / 125, 1 / 60, 1 / 30, 1 / 15, 1 / 8, 1 / 4, 1 / 2, 1],
  APERTURES: [1.4, 2, 2.8, 4, 5.6, 8, 11, 16, 22],
  ISOS: [100, 200, 400, 800, 1600, 3200, 6400, 12800],
  FOCALS: [16, 24, 35, 50, 85, 135, 200],
  LIGHT: { sunny: 15, cloudy: 12, indoors: 8, dusk: 5 },
};

const cam = { shutter: 3, aperture: 5, iso: 0, focal: 3, light: "sunny", walk: false, focus: 6, raf: 0, seed: 1 };

function fmtShutter(t) {
  return t >= 1 ? `${t} s` : `1/${Math.round(1 / t)} s`;
}

// The scene, in metres: x across (0 ahead), y up from the ground, at
// distance d. Each layer draws itself with `p(x, y)` → canvas pixels.
function camScene(subject) {
  return [
    { d: 3000, name: "sky", draw: (g, p, s) => {
      const grad = g.createLinearGradient(0, 0, 0, CAM.H);
      grad.addColorStop(0, "#5b8fd6");
      grad.addColorStop(1, "#cfe3f5");
      g.fillStyle = grad;
      g.fillRect(0, 0, CAM.W, CAM.H);
      g.fillStyle = "#fff8d0";
      const [sx, sy] = p(-700, 450);
      g.beginPath();
      g.arc(sx, sy, Math.max(4, 40 * s), 0, 7);
      g.fill();
    } },
    { d: 1500, name: "hills", draw: (g, p) => {
      g.fillStyle = "#6d8aa0";
      g.beginPath();
      const [x0, y0] = p(-3000, 0);
      g.moveTo(x0, y0);
      for (let x = -3000; x <= 3000; x += 100) {
        const [px, py] = p(x, 120 + 90 * Math.sin(x / 400) + 50 * Math.sin(x / 130));
        g.lineTo(px, py);
      }
      const [x1, y1] = p(3000, 0);
      g.lineTo(x1, y1);
      g.fill();
    } },
    { d: 40, name: "trees", draw: (g, p) => {
      const [, gy] = p(-200, 0);
      g.fillStyle = "#7fa860";
      g.fillRect(0, gy, CAM.W, CAM.H - gy);
      for (let i = -12; i <= 12; i++) {
        const x = i * 6 + (i % 3) * 1.3;
        const h = 9 + (Math.abs(i * 7) % 5);
        const [tx, ty] = p(x, 0);
        const [, top] = p(x, h);
        const [rx] = p(x + 2.2, 0);
        const r = rx - tx;
        g.fillStyle = "#5a3d24";
        g.fillRect(tx - r * 0.12, top + r, r * 0.24, ty - top - r);
        g.fillStyle = i % 2 ? "#2f6b34" : "#3d7d3a";
        g.beginPath();
        g.ellipse(tx, top + r, r, r * 1.3, 0, 0, 7);
        g.fill();
      }
    } },
    { d: 9, name: "ball", moving: true, draw: (g, p) => {
      const [bx, by] = p(1.6, 0.35);
      const [ex] = p(1.95, 0.35);
      const r = ex - bx;
      g.fillStyle = "#e8452c";
      g.beginPath();
      g.arc(bx, by, r, 0, 7);
      g.fill();
      g.fillStyle = "#fff";
      g.fillRect(bx - r, by - r * 0.15, 2 * r, r * 0.3);
    } },
    { d: subject, name: "person", draw: (g, p) => {
      const [x, y] = p(-0.4, 0);
      const [, head] = p(-0.4, 1.6);
      const [x2] = p(-0.15, 0);
      const u = x2 - x;
      g.fillStyle = "#28344e";
      g.fillRect(x - u * 0.7, head + u * 2.2, u * 1.4, (y - head) * 0.45);
      g.fillStyle = "#3a5f9a";
      g.fillRect(x - u * 1.1, head + u * 1.1, u * 2.2, (y - head) * 0.42);
      g.fillStyle = "#e9b98f";
      g.beginPath();
      g.arc(x, head + u * 0.55, u * 0.6, 0, 7);
      g.fill();
      g.fillStyle = "#3b2516";
      g.beginPath();
      g.arc(x, head + u * 0.35, u * 0.62, Math.PI, 0);
      g.fill();
    } },
    { d: 1.4, name: "grass", draw: (g, p) => {
      // So close its roots are below the picture: blades up from the
      // bottom edge, as tall as they'd look at 1.4 m.
      g.strokeStyle = "#4f8f3a";
      for (let i = 0; i < 70; i++) {
        const x = -1.4 + i * 0.04 + (i % 5) * 0.005;
        const [a] = p(x, 0);
        const [c, top] = p(x + 0.03 * Math.sin(i), 1.5 - 0.02 - (i % 4) * 0.01);
        const [, base] = p(x, 1.5 - 0.06);
        const lift = base - top;
        g.lineWidth = Math.max(1.5, lift * 0.05);
        g.beginPath();
        g.moveTo(a, CAM.H + 2);
        g.lineTo(c, CAM.H - lift);
        g.stroke();
      }
    } },
  ];
}

// A box blur of `r` pixels, three times (close to a Gaussian), along x, y
// or both; `dx` alone blurs along a line (motion).
function camBlur(img, rx, ry) {
  const { width: w, height: h, data } = img;
  const pass = (r, horizontal) => {
    if (r < 0.5) return;
    const n = Math.round(r);
    const src = new Float32Array(data);
    const len = horizontal ? w : h;
    const lines = horizontal ? h : w;
    for (let l = 0; l < lines; l++) {
      for (let c = 0; c < 4; c++) {
        let sum = 0;
        const at = (i) => {
          const k = Math.min(len - 1, Math.max(0, i));
          return src[(horizontal ? l * w + k : k * w + l) * 4 + c];
        };
        for (let i = -n; i <= n; i++) sum += at(i);
        for (let i = 0; i < len; i++) {
          data[(horizontal ? l * w + i : i * w + l) * 4 + c] = sum / (2 * n + 1);
          sum += at(i + n + 1) - at(i - n);
        }
      }
    }
  };
  for (let k = 0; k < 3; k++) {
    pass(rx / 2, true);
    pass(ry / 2, false);
  }
}

// What the settings do, as numbers: for the drawing and the notes.
function camModel() {
  const t = CAM.SHUTTERS[cam.shutter];
  const N = CAM.APERTURES[cam.aperture];
  const iso = CAM.ISOS[cam.iso];
  const f = CAM.FOCALS[cam.focal];
  // Walking back: the person stays the size they are at 50 mm and 6 m.
  const subject = cam.walk ? (cam.focus * f) / 50 : cam.focus;
  const ev = Math.log2((N * N) / t) - Math.log2(iso / 100);
  const off = CAM.LIGHT[cam.light] - ev; // + is brighter than right
  const pxPerMm = CAM.W / CAM.SENSOR;
  const coc = (D) => ((f * f) / (N * (subject * 1000 - f))) * (Math.abs(D - subject) / D) * pxPerMm;
  // The ball rolls at 3 m/s at 9 m.
  const motion = ((3 / 9) * f) / CAM.SENSOR * CAM.W * t;
  // Shake: about a hundredth of the field of view a second, handheld.
  const shake = t > 1 / f ? (t - 1 / f) * f * 0.6 : 0;
  return { t, N, iso, f, subject, ev, off, coc, motion, shake };
}

function camDraw() {
  const m = camModel();
  const c = $("camsim-canvas");
  const g = c.getContext("2d");
  const scale = m.f / CAM.SENSOR * CAM.W; // pixels per metre at 1 m
  const horizon = CAM.H * 0.62;
  const out = document.createElement("canvas");
  out.width = CAM.W;
  out.height = CAM.H;
  const og = out.getContext("2d");
  for (const layer of camScene(m.subject)) {
    const lc = document.createElement("canvas");
    lc.width = CAM.W;
    lc.height = CAM.H;
    const lg = lc.getContext("2d");
    // Eye height 1.5 m; things far away sit at the horizon.
    const p = (x, y) => [CAM.W / 2 + (x / layer.d) * scale, horizon - ((y - 1.5) / layer.d) * scale];
    layer.draw(lg, p, scale / 1000);
    const blur = Math.min(40, m.coc(layer.d));
    const motion = layer.moving ? Math.min(120, m.motion) : 0;
    if (blur > 0.5 || motion > 0.5) {
      const img = lg.getImageData(0, 0, CAM.W, CAM.H);
      camBlur(img, blur + motion, blur);
      lg.putImageData(img, 0, 0);
    }
    og.drawImage(lc, 0, 0);
  }
  // Exposure, shake and grain on the whole picture.
  const img = og.getImageData(0, 0, CAM.W, CAM.H);
  if (m.shake > 0.5) camBlur(img, Math.min(60, m.shake), Math.min(30, m.shake * 0.5));
  const gain = 2 ** m.off;
  const lin = (v) => (v / 255) ** 2.2;
  const enc = (v) => 255 * Math.min(1, Math.max(0, v)) ** (1 / 2.2);
  const grain = 0.004 * Math.sqrt(m.iso / 100);
  let s = cam.seed;
  const rnd = () => {
    s = (s * 1103515245 + 12345) & 0x7fffffff;
    return s / 0x7fffffff - 0.5;
  };
  const d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const n = grain * rnd() * 2.5;
    for (let k = 0; k < 3; k++) d[i + k] = enc(lin(d[i + k]) * gain + n * (1 + k * 0.1));
  }
  g.putImageData(img, 0, 0);
  // The settings and what they do.
  $("camsim-shutter-v").textContent = fmtShutter(m.t);
  $("camsim-aperture-v").textContent = `f/${m.N}`;
  $("camsim-iso-v").textContent = `ISO ${m.iso}`;
  $("camsim-focal-v").textContent = `${m.f} mm`;
  const stops = Math.round(m.off * 3) / 3;
  const meter = Math.abs(stops) < 0.34 ? "well exposed" : stops > 0 ? `${stops.toFixed(1)} stops too bright` : `${(-stops).toFixed(1)} stops too dark`;
  const notes = [`Exposure: ${meter}.`];
  if (m.motion > 3) notes.push(`The ball moves ${Math.round(m.motion)} px while the shutter is open: motion blur. A faster shutter freezes it.`);
  else notes.push("The ball is frozen by the shutter.");
  if (m.shake > 0.5) notes.push(`Slower than 1/${m.f} s at ${m.f} mm: camera shake blurs everything. Faster shutter, wider lens, or a tripod.`);
  const bg = m.coc(40);
  notes.push(bg > 4 ? `f/${m.N} at ${m.f} mm: shallow depth of field, the trees are soft (${bg.toFixed(0)} px).` : `f/${m.N}: deep depth of field, the trees are sharp too.`);
  if (m.iso >= 1600) notes.push(`ISO ${m.iso}: visible grain (noise); it lets you use a faster shutter or a smaller aperture.`);
  if (cam.walk) notes.push(`Walked back to ${m.subject.toFixed(1)} m: the person stays the same size, the background grows (compression).`);
  $("camsim-notes").textContent = notes.join(" ");
}

function camRedraw() {
  cancelAnimationFrame(cam.raf);
  cam.raf = requestAnimationFrame(camDraw);
}

// Shutter speed for a good exposure at the other settings.
function camAuto() {
  const m = camModel();
  // Too bright by `off` stops: a shutter that many stops faster.
  const want = m.t / 2 ** m.off;
  let best = 0;
  CAM.SHUTTERS.forEach((t, i) => {
    if (Math.abs(Math.log2(t / want)) < Math.abs(Math.log2(CAM.SHUTTERS[best] / want))) best = i;
  });
  cam.shutter = best;
  $("camsim-shutter").value = best;
  camRedraw();
}

function camsimFlow() {
  const dlg = $("camsim-dialog");
  const bind = (id, key, list) => {
    const el = $(id);
    el.max = list.length - 1;
    el.value = cam[key];
    el.oninput = () => {
      cam[key] = Number(el.value);
      camRedraw();
    };
  };
  bind("camsim-shutter", "shutter", CAM.SHUTTERS);
  bind("camsim-aperture", "aperture", CAM.APERTURES);
  bind("camsim-iso", "iso", CAM.ISOS);
  bind("camsim-focal", "focal", CAM.FOCALS);
  $("camsim-light").value = cam.light;
  $("camsim-light").onchange = (e) => {
    cam.light = e.target.value;
    camRedraw();
  };
  $("camsim-walk").checked = cam.walk;
  $("camsim-walk").onchange = (e) => {
    cam.walk = e.target.checked;
    camRedraw();
  };
  $("camsim-auto").onclick = (e) => {
    e.preventDefault();
    camAuto();
  };
  if (!dlg.open) dlg.showModal();
  camDraw();
}

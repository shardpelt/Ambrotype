// SPDX-License-Identifier: GPL-3.0-or-later
// Scripted UI tests (`ambrotype --ui-test SCRIPT.js [PHOTO]`, src/uitest.rs).
// The script is the body of an async function given `t`, the helpers below;
// it acts like a person would (clicks, keys, sliders) and checks what the
// page shows. Any uncaught error or console.error fails the run.

function uiTestHelpers(log) {
  const t = {
    sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
    // Wait until `cond()` is truthy (and return it), or fail after `ms`.
    async until(what, cond, ms = 20000) {
      const end = Date.now() + ms;
      for (;;) {
        let v;
        try { v = await cond(); } catch { v = null; }
        if (v) return v;
        if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
        await t.sleep(50);
      }
    },
    expect(ok, what) {
      if (!ok) throw new Error(`expected ${what}`);
    },
    el(sel) {
      const e = typeof sel === "string" ? document.querySelector(sel) : sel;
      if (!e) throw new Error(`no element ${sel}`);
      return e;
    },
    visible(sel) {
      const e = typeof sel === "string" ? document.querySelector(sel) : sel;
      return !!e && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== "hidden";
    },
    click(sel) {
      const e = t.el(sel);
      e.scrollIntoView({ block: "center" });
      e.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
      e.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
      e.click();
    },
    // A shortcut as keys.js writes it: "Ctrl+Z", "Shift+Tab", "e".
    key(combo) {
      const parts = combo.split("+");
      const k = parts.pop();
      const mods = new Set(parts.map((p) => p.toLowerCase()));
      const key = k.length === 1 ? (mods.has("shift") ? k.toUpperCase() : k.toLowerCase()) : k;
      const code = k.length === 1 && /[a-z]/i.test(k) ? `Key${k.toUpperCase()}` : k;
      const ev = { key, code, bubbles: true, cancelable: true, ctrlKey: mods.has("ctrl"), metaKey: mods.has("cmd"), shiftKey: mods.has("shift"), altKey: mods.has("alt") };
      (document.activeElement || document.body).dispatchEvent(new KeyboardEvent("keydown", ev));
      (document.activeElement || document.body).dispatchEvent(new KeyboardEvent("keyup", ev));
    },
    // A develop setting's row in the right panel (its section opened).
    row(name) {
      const r = document.querySelector(`.row[data-name="${name}"]`);
      if (!r) throw new Error(`no control for ${name}`);
      return r;
    },
    // Drag a slider to `value` as a person would: input events, then change.
    setSlider(name, value) {
      const input = t.row(name).querySelector('input[type="range"]');
      if (!input) throw new Error(`${name} is not a slider`);
      input.value = String(value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    },
    setting: (name) => ui.settings?.[name],
    historyLength: () => ui.state?.history?.length ?? 0,
    status: () => $("status").textContent,
    invoke,
    log,
  };
  return t;
}

async function runUiTest(test) {
  const report = [];
  let failed = null;
  const fail = (what) => { failed ??= what; };
  const consoleError = console.error;
  console.error = (...a) => { fail(`console.error: ${a.join(" ")}`); consoleError(...a); };
  window.addEventListener("error", (e) => fail(`uncaught: ${e.message}`));
  window.addEventListener("unhandledrejection", (e) => fail(`unhandled: ${e.reason}`));
  // Steps: `t.step("name", async () => …)` reports each one.
  const t = uiTestHelpers((line) => report.push(`  ${line}`));
  t.step = async (name, fn) => {
    if (failed) return;
    const start = performance.now();
    try {
      await fn();
      if (failed) throw new Error(failed);
      report.push(`ok   ${name} (${Math.round(performance.now() - start)} ms)`);
    } catch (e) {
      fail(String(e?.message ?? e));
      report.push(`FAIL ${name}: ${failed}`);
    }
  };
  try {
    // Put in by src/uitest.rs (the page may not eval).
    const body = await t.until("the test script", () => window.__ambUiTest, 10000);
    await body(t);
  } catch (e) {
    fail(String(e?.message ?? e));
    report.push(`FAIL (script): ${failed}`);
  }
  await invoke("ui_test_done", { passed: !failed, report });
}

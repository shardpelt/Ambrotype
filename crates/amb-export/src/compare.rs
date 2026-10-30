// SPDX-License-Identifier: GPL-3.0-or-later
//! A before/after page to share: one HTML file with both photos inside it and
//! a bar to drag between them. Nothing is loaded from elsewhere, so it opens
//! offline, from a mail attachment or any web host.

/// The page for two JPEGs of the same size (`before`, `after`), `width` ×
/// `height` pixels, titled `title`.
pub fn page(title: &str, before: &[u8], after: &[u8], width: usize, height: usize) -> String {
    let title = escape(title);
    let (b, a) = (base64(before), base64(after));
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}: before and after</title>
<style>
  :root {{ color-scheme: dark; }}
  body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: #111214; color: #ddd; font: 14px/1.4 system-ui, sans-serif; }}
  main {{ width: min(100vw - 32px, {width}px, (100vh - 96px) * {width} / {height}); margin: 16px; }}
  h1 {{ font-size: 15px; font-weight: 600; margin: 0 0 10px; }}
  .compare {{ position: relative; aspect-ratio: {width} / {height}; overflow: hidden; border-radius: 4px; cursor: ew-resize; touch-action: none; user-select: none; }}
  .compare img {{ position: absolute; inset: 0; width: 100%; height: 100%; display: block; pointer-events: none; }}
  .compare .before {{ clip-path: inset(0 50% 0 0); }}
  .bar {{ position: absolute; top: 0; bottom: 0; left: 50%; width: 2px; margin-left: -1px; background: #fff; box-shadow: 0 0 4px #0009; pointer-events: none; }}
  .bar::after {{ content: "\2194"; position: absolute; top: 50%; left: 50%; width: 30px; height: 30px; transform: translate(-50%, -50%); border-radius: 50%; background: #fff; color: #222; display: grid; place-items: center; font-size: 16px; box-shadow: 0 1px 6px #0008; }}
  .label {{ position: absolute; top: 10px; padding: 3px 9px; border-radius: 10px; background: #0009; color: #fff; font-size: 12px; pointer-events: none; }}
  .label.b {{ left: 10px; }} .label.a {{ right: 10px; }}
  footer {{ margin-top: 8px; font-size: 12px; color: #888; }}
  footer a {{ color: #aaa; }}
</style>
</head>
<body>
<main>
  <h1>{title}</h1>
  <div class="compare" role="slider" aria-label="Before and after" aria-valuemin="0" aria-valuemax="100" aria-valuenow="50" tabindex="0">
    <img class="after" alt="After" src="data:image/jpeg;base64,{a}">
    <img class="before" alt="Before" src="data:image/jpeg;base64,{b}">
    <span class="label b">Before</span><span class="label a">After</span>
    <div class="bar"></div>
  </div>
  <footer>Drag across the photo, or use the arrow keys. Edited with <a href="https://github.com/shardpelt/Ambrotype">Ambrotype</a>.</footer>
</main>
<script>
  const box = document.querySelector(".compare"), before = box.querySelector(".before"), bar = box.querySelector(".bar");
  const set = (f) => {{
    f = Math.min(1, Math.max(0, f));
    before.style.clipPath = `inset(0 ${{(1 - f) * 100}}% 0 0)`;
    bar.style.left = `${{f * 100}}%`;
    box.setAttribute("aria-valuenow", Math.round(f * 100));
  }};
  const at = (e) => set((e.clientX - box.getBoundingClientRect().left) / box.clientWidth);
  box.addEventListener("pointerdown", (e) => {{ box.setPointerCapture(e.pointerId); at(e); box.onpointermove = at; }});
  box.addEventListener("pointerup", () => {{ box.onpointermove = null; }});
  box.addEventListener("keydown", (e) => {{
    const now = Number(box.getAttribute("aria-valuenow")) / 100;
    if (e.key === "ArrowLeft") set(now - 0.05);
    else if (e.key === "ArrowRight") set(now + 0.05);
  }});
</script>
</body>
</html>
"#
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_page_holds_both_photos_and_no_outside_files() {
        let p = page("Harbour <dusk>", b"before", b"after!", 300, 200);
        assert!(p.contains("Harbour &lt;dusk&gt;"));
        assert!(p.contains(&base64(b"before")) && p.contains(&base64(b"after!")));
        assert!(p.contains("aspect-ratio: 300 / 200"));
        assert!(
            !p.contains("src=\"http") && !p.contains("<link"),
            "nothing loaded from elsewhere"
        );
    }
}

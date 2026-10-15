// SPDX-License-Identifier: GPL-3.0-or-later
// Toolbar icons: simple line drawings (our own), drawn in the text colour,
// for the buttons in TOOLBAR_ICONS. Drawings for the others are kept here in
// case a layout wants them later.
// The button keeps its name for screen readers (aria-label) and its tooltip.
"use strict";

const ICON_PATHS = {
  // A folder, opened.
  open: '<path d="M3 18V6.5A1.5 1.5 0 0 1 4.5 5h4l2 2h7A1.5 1.5 0 0 1 19 8.5V10"/><path d="M3 18l2.6-6.6A1.5 1.5 0 0 1 7 10.5h13.4a1 1 0 0 1 .9 1.4L18.8 18a1.5 1.5 0 0 1-1.4 1H4.4A1.4 1.4 0 0 1 3 18Z"/>',
  // A grid of photos.
  library: '<rect x="3.5" y="3.5" width="7" height="7" rx="1"/><rect x="13.5" y="3.5" width="7" height="7" rx="1"/><rect x="3.5" y="13.5" width="7" height="7" rx="1"/><rect x="13.5" y="13.5" width="7" height="7" rx="1"/>',
  // A floppy disk.
  save: '<path d="M5 3.5h11.5l4 4V19a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 19V5A1.5 1.5 0 0 1 5 3.5Z"/><path d="M7.5 3.5v5h8v-5"/><rect x="7" y="13" width="10" height="7.5" rx=".5"/>',
  // Out of the box.
  export: '<path d="M12 3v12"/><path d="M7.5 7.5 12 3l4.5 4.5"/><path d="M8 11H5.5A1.5 1.5 0 0 0 4 12.5v6A1.5 1.5 0 0 0 5.5 20h13a1.5 1.5 0 0 0 1.5-1.5v-6a1.5 1.5 0 0 0-1.5-1.5H16"/>',
  // Photos side by side on one page.
  collage: '<rect x="3.5" y="3.5" width="17" height="17" rx="1.5"/><path d="M11 3.5v17M11 12h9.5"/>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H10"/>',
  redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H14"/>',
  // Half before, half after.
  before: '<rect x="3.5" y="4.5" width="17" height="15" rx="1.5"/><path d="M12 2.5v19"/><path d="M12 4.5h7a1.5 1.5 0 0 1 1.5 1.5v12a1.5 1.5 0 0 1-1.5 1.5h-7Z" fill="currentColor" stroke="none" opacity=".45"/>',
  // A selection with handles at the corners.
  crop: '<rect x="5" y="5" width="14" height="14" stroke-dasharray="2.6 2.2"/><rect x="3" y="3" width="4" height="4" fill="currentColor" stroke="none"/><rect x="17" y="3" width="4" height="4" fill="currentColor" stroke="none"/><rect x="3" y="17" width="4" height="4" fill="currentColor" stroke="none"/><rect x="17" y="17" width="4" height="4" fill="currentColor" stroke="none"/>',
  // A masquerade mask.
  masks: '<path d="M2.5 8.5C2.5 6.8 4.5 6 7 6c2.2 0 3.4 1 5 1s2.8-1 5-1c2.5 0 4.5.8 4.5 2.5 0 4.6-2.4 8-5.2 8-2.1 0-2.6-2.2-4.3-2.2s-2.2 2.2-4.3 2.2c-2.8 0-5.2-3.4-5.2-8Z"/><ellipse cx="7.8" cy="10.3" rx="2.1" ry="1.4"/><ellipse cx="16.2" cy="10.3" rx="2.1" ry="1.4"/>',
  // A plaster: healing.
  spots: '<rect x="2.6" y="8.2" width="18.8" height="7.6" rx="3.8" transform="rotate(-45 12 12)"/><rect x="8.6" y="8.6" width="6.8" height="6.8" transform="rotate(-45 12 12)"/><circle cx="11" cy="12" r=".6" fill="currentColor"/><circle cx="13" cy="12" r=".6" fill="currentColor"/><circle cx="12" cy="11" r=".6" fill="currentColor"/><circle cx="12" cy="13" r=".6" fill="currentColor"/>',
  // A magnifier.
  zoom: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 5 5"/><path d="M10.5 7.5v6M7.5 10.5h6"/>',
  // An eyedropper.
  wb: '<path d="m14.5 4.5 5 5"/><path d="M13 6l5 5-8.5 8.5H5.5v-4L13 6Z"/><path d="M16 3a2.1 2.1 0 0 1 3 0l2 2a2.1 2.1 0 0 1 0 3l-1.5 1.5-5-5Z"/>',
  // A warning triangle.
  clip: '<path d="M12 3.5 21.5 20h-19L12 3.5Z"/><path d="M12 10v4.5"/><circle cx="12" cy="17.3" r=".8" fill="currentColor" stroke="none"/>',
  // A picture in a frame.
  frame: '<rect x="3" y="3" width="18" height="18" rx="1"/><rect x="6.5" y="6.5" width="11" height="9"/><path d="m6.5 14 3.2-3.2 2.8 2.8 2-2 3 3"/>',
  // A keyboard.
  keys: '<rect x="2.5" y="6" width="19" height="12" rx="1.5"/><path d="M6 9.5h.01M9 9.5h.01M12 9.5h.01M15 9.5h.01M18 9.5h.01M6 12h.01M9 12h.01M12 12h.01M15 12h.01M18 12h.01M8 15h8"/>',
  // A puzzle piece.
  plugins: '<path d="M10 4.5a2 2 0 1 1 4 0V6h4a1 1 0 0 1 1 1v4h-1.5a2 2 0 1 0 0 4H19v4a1 1 0 0 1-1 1h-4v-1.5a2 2 0 1 0-4 0V20H6a1 1 0 0 1-1-1v-4h1.5a2 2 0 1 0 0-4H5V7a1 1 0 0 1 1-1h4Z"/>',
  // A painter's palette.
  looks: '<path d="M12 3a9 9 0 1 0 0 18c1.3 0 2-.8 2-1.8 0-.5-.2-.9-.5-1.3-.3-.4-.5-.8-.5-1.3 0-1 .8-1.6 1.8-1.6H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3Z"/><circle cx="7.5" cy="11" r="1.2" fill="currentColor"/><circle cx="10" cy="7" r="1.2" fill="currentColor"/><circle cx="14.5" cy="7" r="1.2" fill="currentColor"/><circle cx="17.2" cy="10.5" r="1.2" fill="currentColor"/>',
  // Panels being rearranged.
  edit: '<rect x="3.5" y="4.5" width="17" height="15" rx="1.5"/><path d="M9 4.5v15"/><path d="m13 15.5 4-4 1.5 1.5-4 4H13Z"/>',
  // A cog.
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1"/><circle cx="12" cy="12" r="6.5"/>',
  // A speech bubble.
  feedback: '<path d="M4.5 5h15A1.5 1.5 0 0 1 21 6.5v9a1.5 1.5 0 0 1-1.5 1.5H10l-4.5 3.5V17h-1A1.5 1.5 0 0 1 3 15.5v-9A1.5 1.5 0 0 1 4.5 5Z"/><path d="M7.5 9.5h9M7.5 12.5h6"/>',
};

// Only the buttons whose icon says what it does at a glance get one; the
// rest keep their word, so the bar stays easy to read.
const TOOLBAR_ICONS = new Set(["open", "save", "undo", "redo", "crop", "masks", "zoom", "wb", "settings"]);

function iconSvg(name) {
  const paths = ICON_PATHS[name];
  if (!paths) return null;
  return `<svg class="tool-svg" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

// Swap the toolbar's words for icons; the name stays as the label for
// screen readers and in the tooltip.
function applyToolbarIcons() {
  for (const [name, id] of Object.entries(TOOLBAR_IDS)) {
    const b = $(id);
    const svg = TOOLBAR_ICONS.has(name) ? iconSvg(name) : null;
    if (!b || !svg || b.classList.contains("tool-icon")) continue;
    const label = b.getAttribute("aria-label") || b.textContent.trim().replace(/…$/, "");
    b.setAttribute("aria-label", label);
    if (!b.title) b.title = label;
    b.innerHTML = svg;
    b.classList.add("tool-icon");
  }
}

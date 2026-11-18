// SPDX-License-Identifier: GPL-3.0-or-later
// The app's words (core/lang.js, docs/design/language.md): `t` and `plural` in
// English, and the made-up language `zz`, which marks every text that went
// through the table. The texts on screen without the marks are not through yet;
// that count may only fall (UNMARKED_LIMIT: lower it when you wrap texts).
// Run: ambrotype --ui-test crates/amb-gui/tests/ui/language.js photo.dng

// The test's own `t` hides the app's, so the app's is `tr` here.
const tr = window.t;
const UNMARKED_LIMIT = 651;

await t.step("t and plural give English with no language", async () => {
  t.expect(lang.code === "en" && lang.texts === null, "English to start");
  t.expect(tr("Moved to the trash") === "Moved to the trash", "the text itself");
  t.expect(tr("Copied as “{name}”", { name: "a" }) === "Copied as “a”", "variables go in");
  t.expect(tr("Open##verb") === "Open", "the hint after ## is left off");
  t.expect(plural(1, "photo") === "1 photo" && plural(3, "photo") === "3 photos", "plural");
});

await t.step("zz marks the markup, t and plural", async () => {
  const before = document.querySelector("#btn-open").title;
  await langSet("zz");
  t.expect(lang.code === "zz", "zz is on");
  t.expect(tr("Open a photo").startsWith("⟦") === (lang.texts["Open a photo"] !== undefined), "t marks what is in the table");
  const title = document.querySelector("#btn-open").title;
  t.expect(!before || title === `⟦${before}⟧` || title === before, `a title is marked or not in the table: ${title}`);
  t.expect(plural(3, "photo") === "⟦3 photos⟧" && plural(1, "photo") === "⟦1 photo⟧", `each plural form is marked: ${plural(3, "photo")}`);
  const left = langUnmarked();
  console.log(`texts not through the table yet: ${left.length}`);
  t.expect(left.length <= UNMARKED_LIMIT, `${left.length} texts on screen are not through the table (limit ${UNMARKED_LIMIT}): ${left.slice(0, 5).join(" | ")}`);
});

await t.step("English again: the markup too", async () => {
  await langSet("en");
  t.expect(lang.texts === null && tr("Hello") === "Hello", "back to English");
  t.expect(langUnmarked().length > 0 && !document.body.textContent.includes("⟦"), "no marked text left on screen");
  t.expect(![...document.querySelectorAll("[title]")].some((e) => e.title.startsWith("⟦")), "no marked title left");
});

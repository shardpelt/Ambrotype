# Legal position

Checked on 2026-10-04 and kept up to date. This is an engineer's reading of the rules, not legal advice. Before 1.0, have an IP lawyer review it.

## Name and trademarks

- **Ambrotype** is the project's name (from 2026-10-05; it was "Flightbroom"): the 1850s photograph on glass. Crates are `amb-*`, the CLI `amb`, the desktop app `ambrotype`, our XMP namespace `amb:` = `http://ns.ambrotype.org/develop/1.0/`, and environment variables `AMB_*`.
- **Other companies' names** are used only to describe compatibility: "Lightroom, Camera Raw and Adobe are trademarks of Adobe Inc.; Ambrotype is not affiliated with or endorsed by Adobe." The full list is in `NOTICE`, the README and the app's About box. No Adobe logos, icons, UI artwork or colour schemes.

## Licence

- **GPL-3.0-or-later**, copyright "The Ambrotype contributors". The plugin SDK and the `amb plugin new` template are MIT OR Apache-2.0, so plugins may use any licence.
- `LICENSE` (the GPL text), `NOTICE` (copyright, exceptions, ported code, library notes, trademarks), and the SDK's `LICENSE-MIT` and `LICENSE-APACHE`.
- `license` in every Cargo.toml and an SPDX line at the top of every source file.
- Every dependency is GPL-compatible (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode-3.0, LGPL-2.1 for rawler). CI checks it on every push (`cargo deny check licenses sources`, allowed list in `deny.toml`).
- AI models aren't shipped: the app downloads them from their authors when the user asks, and lists only models under a licence that allows this (`crates/amb-ai/data/models.json`).

## Releases

1. Ship `LICENSE`, `NOTICE` and the licence texts of all bundled libraries with every installer (`cargo about`; MIT, BSD and Apache require the notices to travel with binaries).
2. Each release points to the exact source it was built from (the Git tag).

## Is what we do legal?

| Activity | Position | Why |
|---|---|---|
| Reading Adobe's public help pages | ✅ Fine | Reading is never infringement. Writing down what features do, in our own words, is fine too. Ideas, functionality and behaviour are not protected by copyright. |
| Publishing notes about them | ✅ Fine, kept short | The research notes and design docs are in our own words. A scan on 2026-10-11 found no line sharing a run of 12 or more words with Adobe's help pages, specs or SDK sources; the only quotes are short UI labels, menu names and document titles. The maintainers' detailed digests of Adobe's help and SDKs aren't published (since 2026-10-19). Re-run the check before each release. |
| Reading and writing `.xmp`, `.lrtemplate` and DNG files | ✅ Fine | Data formats and functionality are not protected by copyright (CJEU *SAS Institute v World Programming*, C‑406/10). XMP is an open ISO standard (ISO 16684). Adobe publishes the DNG spec with a royalty-free patent licence for readers and writers. That licence is lost if you sue Adobe over DNG patents, which we won't. |
| Using a Lightroom subscription to compare outputs (render, measure, fit our own constants) | ✅ Fine | This is black-box testing: run the program normally and observe its output. Under the EU Software Directive (2009/24/EC art. 5(3)), a lawful user may "observe, study or test the functioning of the program", and art. 8 makes contract terms that forbid this void. Adobe's terms forbid reverse engineering in the decompiling sense, which we never do. |
| Decompiling or disassembling Lightroom/ACR, or extracting its profiles, LUTs or code | ❌ Never | It breaches Adobe's terms, it isn't needed, and it would taint the code. The project rule stands: no Adobe binaries, profiles (DCP), LCP lens files, presets or code in the repo or the downloads. |
| Publishing calibration renders made with Lightroom | ⚠️ Keep private | You own the copyright in your own photos and their renders, but there's no reason to publish bulk Adobe output. Publish only our fitted numbers (`--calibration` JSON), which are our own measurements. |
| Saying "imports Lightroom® presets" | ✅ Fine | Nominative, descriptive use. Add the notice "Lightroom is a trademark of Adobe Inc.; not affiliated with or endorsed by Adobe." Don't use Adobe logos, icons, UI artwork or colour scheme. |
| Users loading presets they bought | ✅ Their right | Users load their own files, as they would in Lightroom. We never distribute purchased presets (and none are in the repo). |
| Porting darktable/RawTherapee/ART code | ✅ Fine under GPL-3.0-or-later | Keep their copyright headers and credit them. RapidRAW is AGPL: read it for ideas only, don't copy code. |
| Patents | ⚠️ Low, not zero | Software patents mostly bite in the US. darktable, RawTherapee and others have shipped similar features for 15+ years without action. We use published academic methods, not Adobe's. Never read Adobe patents "to see how they do it". |
| Reading Adobe's DNG SDK and specifications | ✅ Fine, facts only | Adobe publishes the DNG SDK and the DNG and XMP specifications for anyone writing compatible software. We take facts about file formats from them (how a value or table is stored, the order of a reference render), which copyright doesn't cover, and copy no code. Lightroom and Camera Raw themselves are only ever black-box tested. |
| Donations | ✅ Fine | Donations to an individual can count as taxable income. A fiscal host (such as Open Collective) handles that and keeps the books public. |

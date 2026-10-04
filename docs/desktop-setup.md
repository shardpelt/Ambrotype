# Running Flightbroom on your desktop

## 1. Install and build (Windows, macOS or Linux)
1. Install Rust from <https://rustup.rs>. On Windows, accept the Visual Studio C++ build tools prompt.
2. Get the code:
   ```sh
   git clone https://github.com/shardpelt/flightbroom.git
   cd flightbroom
   git checkout claude/lightroom-alternative-scope-xlnf0p
   ```
3. Build and test:
   ```sh
   cargo test                      # all unit and integration tests should pass
   cargo build --release           # the tool is target/release/flb (flb.exe on Windows)
   ```

## 2. Try it on your own photos
```sh
flb render photo.NEF out.jpg                       # any raw rawler supports (NEF, CR2/CR3, ARW, RAF, DNG, ...)
flb render photo.NEF out.jpg --preset look.xmp --amount 70
flb preset inspect look.xmp                        # what a preset contains and what we can't do yet
flb preset census "D:/Presets"                     # which settings your whole preset collection uses
```
Rendering so far covers only white balance, Exposure and point curves, with a placeholder base look. The command lists what it skipped. If you can, send back the `flb preset census` output for your bought presets. It's only setting names and counts, no preset content, and it tells us what to build first.

## 3. Prepare the Lightroom kit (before subscribing)
Put the kit on the first Kingston SSD and the exports on the second one (see [lightroom-trial-plan.md](lightroom-trial-plan.md)).
```sh
flb sweep plan                                                # the 66 sweeps
flb sweep generate --out E:/kit --synthetic ramp,hue,edges,colorchecker
flb sweep generate --out E:/kit-raw --groups tone,white_balance,presence  D:/raws/*.NEF
```

## 4. Smoke test with Lightroom's 7-day free trial
1. Import a few files from `E:/kit` with **Add**.
2. Check three things:
   - The synthetic DNGs open, showing the profile "Flightbroom Linear ProPhoto".
   - The settings are applied: open `colorchecker__Exposure2012_p1.dng` and Exposure should read +1.00.
   - The proprietary-raw copies pick up their `.xmp` sidecars.
3. Note the Lightroom version and the process version it shows.
4. Report anything that doesn't import before starting the paid month.

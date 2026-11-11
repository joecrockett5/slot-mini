# slot-mini: build prompt

## Goal

Build **slot-mini**: a game launcher for the Miyoo Mini Plus running Onion OS that recreates the look and feel of [slot](https://github.com/BrandonKowalski/slot) for GBA, GB and GBC games. Take this repository, which holds only this prompt and slot, to a release zip a user can copy onto their SD card. Work through every phase below without stopping for approval; stop only for the decisions listed under Working rules.

## Context

slot is a bespoke frontend for the Anbernic RG SP (Allwinner H700, aarch64, Mali GPU). It is about 128,000 lines of Rust across eight crates in `crates/`: `slot` (app), `slot-ui` (shelf, carts, labels, menus, HUD), `slot-gfx` (OpenGL ES rendering, bezels, LCD3x mask), `slot-input`, `slot-power`, `slot-retro` (in-process libretro host), `slot-store` and `slot-sfxcut`. Art lives in `assets/` and the user-facing tour in `site/docs/tour.md`. It was pinned as a git submodule at `reference/slot` (slot `e66715d`) during the build-out; it has since been removed, so clone https://github.com/BrandonKowalski/slot outside the repo when you need it. Treat it as read-only reference: copy from it, never build it or link to it.

slot is licensed **GPL-3.0-or-later** (the workspace `Cargo.toml`; the README says MIT, but the code is GPL). Anything you copy or adapt makes slot-mini GPL-3.0-or-later too. Keep slot's copyright notices, credit it in the README, and ship its licence. The Open Sans and Nerd Fonts licences come along with the fonts.

The target differs from slot's in every layer that matters:

|  | slot on RG SP | slot-mini on Miyoo Mini Plus |
| --- | --- | --- |
| CPU | H700, 64-bit aarch64 | SigmaStar SSD202D, 32-bit ARMv7 Cortex-A7 |
| RAM | 1 GB | 128 MB |
| Graphics | OpenGL ES 3 | No GL; framebuffer plus a 2D blitter |
| Screen | Laid out at 720x480 (3:2); adapts to 640x480 on the RG35XX SP | 640x480 (4:3) |
| OS layer | AGS-102 (BaseOS fork), slot boots directly | Onion OS on top of Miyoo's stock firmware |
| Emulation | In-process mGBA, gpSP, Gambatte | Onion's RetroArch, launched as a separate process |

slot already handles a 640x480 panel (see slot-gfx's `surface.rs` and `fbdev.rs`, and the 4:3 bezels), so its 4:3 layout is your reference, not the 720x480 one. Everything that touches GL, the emulator loop, Wi-Fi link play, the web server or RomM does not.

## Device and Onion: leads to verify

The points below come from memory and community docs, not a device. Confirm each one against source before you build on it, and record what you find in `docs/PLATFORM.md` with links. Two codebases are your ground truth:

- [Onion OS](https://github.com/OnionUI/Onion): open source apart from MainUI. Read its boot loop, launch scripts, key maps, battery and audio code.
- [Open MainUI](https://github.com/robcodedev/onionos-mainui-opensource): a working, GPL-3.0 C replacement for MainUI, released 2026. It solves the same integration problem you face, so read its `docs/BUILDING.md`, how it launches games, and how it updates Recents.

What to verify:

- **Display:** `/dev/fb0`, 640x480, 32 bpp. The panel may be mounted rotated 180°; check how Onion and Open MainUI flip it. Page-flip with `FBIOPAN_DISPLAY` to avoid tearing. The `MI_GFX` blitter in `libmi_gfx.so` is optional speed-up; start in software.
- **Input:** evdev on `/dev/input/event*`. Take key codes from Onion's key map headers, including MENU, POWER, L2 and R2.
- **Audio:** stock audio goes through SigmaStar's `MI_AO`. Find the simplest working way to play a short PCM clip; if it fights you, ship without sound and say so.
- **Battery and Wi-Fi:** find how Onion reads battery level and Wi-Fi state on the Mini Plus.
- **Launch flow:** Onion's runtime loop starts MainUI, which writes a command file (believed `/tmp/cmd_to_run.sh`) and exits; the loop runs it, then relaunches MainUI. Confirm the file name and the script it calls.
- **Recents and Game Switcher:** find how MainUI updates `recentlist.json` so Onion's Game Switcher and auto-resume keep working after a launch from slot-mini.
- **Paths:** ROMs in `/mnt/SDCARD/Roms/{GBA,GB,GBC}`, launch scripts in `/mnt/SDCARD/Emu/<system>/launch.sh`, RetroArch in `/mnt/SDCARD/RetroArch`, saves under `/mnt/SDCARD/Saves/CurrentProfile/`. Confirm all of them.
- **Resume vs fresh start:** Onion has RetroArch auto-save a state on exit and auto-load it on launch. Tap A should launch normally. Hold A should launch with auto-load turned off for that run only (for example a temporary `--appendconfig` file). Never delete or overwrite a user's save states.
- **Toolchain:** Onion builds against an old glibc. Prefer a static `armv7-unknown-linux-musleabihf` binary with `target-cpu=cortex-a7` and NEON so libc versions cannot bite. Onion's Docker toolchain is a fallback if your sandbox has Docker. Check every build with `file`.

## Scope

slot-mini is a launcher. When a game starts, slot-mini exits and Onion's RetroArch takes over, with its own menu, save states, fast-forward and shaders. Match slot's design as closely as the hardware allows; where you must simplify, keep the silhouette, colours and motion and drop the expensive detail.

### In scope

- **Carousel:** slot's shelf of GBA, GB and GBC cartridges, each in its own shell art from `assets/`, with the label drawn on the cart face, the slot chrome and the footer button legend.
- **Controls:** match slot's tour where the Mini Plus has the button.

| Input | Action |
| --- | --- |
| Left / Right | Browse the carousel |
| L1 / R1 | Switch between GBA, GB and GBC |
| Tap A | Launch and resume the auto save state |
| Hold A | Launch fresh, without loading a state |
| Y | Toggle favourite on this cart |
| SELECT | Open the cart studio |
| MENU | About screen, with an option to exit to Onion |

- **Favourites:** hearted carts sort first or get their own shelf, whichever slot does; store them in slot-mini's own data folder.
- **Cart Studio compatibility:** slot-mini must read exactly the files slot's browser-based [Cart Studio](https://studio.slot-cfw.fyi) writes, so labels and shells designed there show up with no conversion:
  - `Labels/{GBA,GB,GBC}/<rom stem>.png` at the SD card root, scaled to cover and centre-cropped the way slot does (GBA carts 1.94:1, GB and GBC 1.12:1)
  - `Labels/cart_shell.txt`, one line per game: `<rom stem> = <shape> <hex colour> <finish>`, with shape `auto`, `notched` or `rounded` and finish `solid`, `clear` or `glitter`; unreadable lines are skipped
  - Copy slot's parsers for both rather than reimplementing them; `site/docs/guide/customizing.md` documents the formats.
- **On-device cart studio:** SELECT opens it for the current cart. Change shell shape, colour and finish, written back to `Labels/cart_shell.txt` in the same format, and print a new label over Wi-Fi.
- **Label service:** slot's art service, `art.slot-cfw.fyi`, is a static host. Its `index.json` (fetched with ETag caching) maps a ROM's CRC32 to a scanned real label, a game logo and a background colour; the images are `.webp`. Port slot's client from `crates/slot/src/art/` (`index.rs`, `jobs.rs`, `make.rs`, `rom.rs`), including how it builds logo-only labels. Put it behind a config flag, off by default, until its author approves third-party use. With it off, the print option says labels can be made in Cart Studio on a computer. Local label files always win.
- **Look:** a random wallpaper from `Wallpapers/` each boot, `theme.txt` colour overrides in slot's format, and slot's cartridge insert and eject sounds.
- **HUD:** battery level and clock, if Onion exposes them cheaply.
- **Files:** read ROMs from Onion's `Roms/` folders; never require users to move games. Keep `Labels/` at the SD root in slot's layout so Cart Studio output drops straight in. Keep everything else of slot-mini's (settings, theme, favourites, cache, logs) in its app folder. Cart Studio looks for a `Games/` folder beside `Labels/`, which Onion does not have; document the current workaround in the README and treat it as an open question.

### Out of scope

In-process emulation, slot's in-game menu and save-state switcher, rewind and fast-forward bindings, the core picker, shaders, bezels and colour grading, link play, the web companion, RomM, QR codes, lid handling, brightness and blue-light controls, boot logos, and systems other than GBA, GB and GBC. Note any of these that would be easy follow-ups in the README's roadmap, but do not build them.

## Architecture

**Reuse slot by copying, not depending.** Start a new Cargo workspace. Copy in only the slot code you need (cart and shelf layout, labels, text, palette, theme parsing, ROM scanning and name cleaning, hold and tap detection), then trim it. Put a provenance line at the top of each copied file: slot commit, original path, and what you changed. Do not pull slot in as a git dependency; its crates bring winit, glutin, GL, cpal and gilrs, none of which belong on this device.

**Software rendering behind one interface.** slot-ui draws through slot-gfx's GL pipeline, so replace that seam with a small `Canvas` over a 640x480 ARGB8888 buffer: fill, alpha blit, pre-scaled blit and text through fontdue with a glyph cache. Implement three backends:

- `fbdev` for the device, double-buffered with page flips
- a desktop window (minifb or softbuffer) for development
- headless PNG output for tests

**Bake what slot did in shaders.** Effects such as the LCD3x mask, colour grading, blur and sticker curl either become pre-rendered PNGs made at build time by a host-side tool, or are dropped. List each one and what you chose in `docs/DESIGN-PARITY.md`.

**Budgets on the device:**

- First frame within 1 second of launch. Onion relaunches the launcher after every game, so cold start is felt constantly.
- Steady 60 fps while scrolling. Composite pre-rendered cart sprites (shell, label and finish merged once) rather than drawing carts from parts each frame.
- Under 40 MB resident memory. Keep only nearby carts decoded, and cache scaled labels on the SD card.
- Return to the cart you just played after a game exits.

**Networking:** blocking `ureq` with rustls and `webpki-roots` on a worker thread; no async runtime. Check that it cross-compiles for musl armv7 early.

**Install and the switch:**

- Ship as an Onion app in `App/slot-mini/` with the manifest and `launch.sh` Onion expects; check the format against an existing Onion app.
- An in-app setting, **Use slot-mini as the boot launcher**, installs a small shell shim in place of Onion's MainUI binaries in `.tmp_update/bin/`, after backing up each original once. Open MainUI's install guide names the binaries for each hardware variant.
- The shim runs slot-mini and falls back to the original MainUI if slot-mini is missing, exits with an error, or the user holds a button at boot. A matching **Restore Onion launcher** setting puts the originals back.
- No sequence of crashes, half-finished installs or removed files may leave the device without a working launcher. Treat this as the top-priority requirement and test it.

## Plan

Work in these phases, in order. Finish each phase's exit check before starting the next, commit at every exit, and keep `docs/PROGRESS.md` updated with what is done, what is next and any open questions.

1. **Research.** Clone slot, Onion and Open MainUI into a scratch folder outside the repo. Write `docs/PLATFORM.md` (the verified facts above, with source links) and `docs/DESIGN-PARITY.md` (every slot screen and effect in scope, and how you will reproduce it). Get reference images of slot's carousel at 640x480: try slot's own headless render tests first; if slot will not render in your sandbox, use `assets/` and the screenshots and clips under `site/docs/`.
   - Exit: both docs exist and every lead in the Device section is marked confirmed, corrected or unresolved.
2. **Skeleton and toolchain.** Workspace, `Canvas`, desktop and headless backends, and a static armv7 musl release build, also running in GitHub Actions.
   - Exit: CI is green and `file` reports a static 32-bit ARM binary.
3. **Carousel.** ROM scanning, cart sprites, shelf, scroll animation, platform switching, footer, wallpaper and `theme.txt`.
   - Exit: golden-image tests pass, and `docs/DESIGN-PARITY.md` shows slot's reference and slot-mini's render side by side for each screen.
4. **Launching.** Onion's launch flow, tap and hold A, Recents updates and returning to the last cart. Build a fake SD card tree and a stand-in for Onion's runtime loop so the whole cycle runs on the desktop.
   - Exit: an integration test launches a game through the fake loop, checks the command written, and relaunches onto the same cart.
5. **Extras.** Favourites, cart studio, label fetching (local files first, then slot's service when enabled), sounds, battery and clock.
   - Exit: each has tests; network code is tested against a local test server, not the internet.
6. **Device backends.** fbdev, evdev input, audio, battery and Wi-Fi status. You cannot run these on hardware, so test what you can off-device: evdev parsing from recorded event bytes, fbdev drawing into a temporary file sized like the framebuffer.
   - Exit: the device build compiles cleanly with these backends selected.
7. **Packaging and the switch.** Release zip, app manifest, `launch.sh`, the boot shim, install and restore. Write shell tests for the shim in the style of slot's `card/test-*.sh`: crash fallback, missing binary, button-held boot, double install and restore.
   - Exit: all shim tests pass and a release zip is built by CI.
8. **Docs.** README with install, uninstall, controls, credits, licence and an AI disclosure like slot's. A `docs/DEVICE-TESTING.md` checklist for a human to run on a real Mini Plus, ordered so the safest checks (app mode) come before the boot switch.

## Testing

No device is available during this run, so the desktop and headless builds are your device. Write tests as you go, not at the end:

- Golden-image tests for every screen, rendered headless at 640x480. Commit the goldens, and when one changes, say why in the commit.
- Unit tests for ROM scanning, name cleaning, theme parsing, favourites, hold-A timing and the label source fallback order.
- The integration test from phase 4 and the shim tests from phase 7.
- A host-side performance check: time a full carousel scroll and first-frame start-up on the desktop build, and record numbers in `docs/PROGRESS.md`. Flag anything that would clearly miss the device budgets on a slow 32-bit chip.

Look at your rendered PNGs yourself before calling a screen done. A passing golden test only proves the image did not change.

## Done means

- [ ] CI builds and tests the desktop build and the armv7 release on every push
- [ ] The release zip installs as an Onion app with no other changes to the card
- [ ] Every in-scope feature works on the desktop build against a fake Onion SD tree
- [ ] The boot switch, its fallback and the restore all pass their tests
- [ ] `PLATFORM.md`, `DESIGN-PARITY.md`, `PROGRESS.md`, `DEVICE-TESTING.md` and the README are complete
- [ ] Licences and credits for slot, fonts and any other copied work ship in the zip

## Working rules

- Work autonomously through all phases. Make reasonable calls, record them in `docs/DECISIONS.md` with the reason, and keep going.
- Stop and ask only if: a needed fact about Onion or the hardware cannot be confirmed from source and guessing could stop a device booting; slot's licence or its art service's terms would block a planned feature; or a budget looks impossible without cutting scope.
- Never write anything that runs on first boot outside `App/slot-mini/` except through the opt-in switch.
- Keep commits small and named by phase. Do not reformat or restructure copied slot code beyond what you need.
- If you run low on context, update `docs/PROGRESS.md` first so a fresh session can pick up from it.

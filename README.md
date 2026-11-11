<p align="center">
  <img src="docs/media/banner.png" alt="slot-mini: a cartridge-shelf launcher for the Miyoo Mini Plus" width="100%">
</p>

<p align="center">
  <a href="https://github.com/joecrockett5/slot-mini/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/joecrockett5/slot-mini/ci.yml?branch=main&label=CI&logo=githubactions&logoColor=white"></a>
  <a href="https://github.com/joecrockett5/slot-mini/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/joecrockett5/slot-mini?label=release&color=2f5cc0"></a>
  <a href="LICENSE"><img alt="Licence: GPL-3.0-or-later" src="https://img.shields.io/badge/licence-GPL--3.0--or--later-3a7d44"></a>
  <img alt="Device: Miyoo Mini Plus" src="https://img.shields.io/badge/device-Miyoo%20Mini%20Plus-c23b22">
  <a href="https://github.com/OnionUI/Onion"><img alt="Onion OS 4.3.1-1 and development build" src="https://img.shields.io/badge/Onion%20OS-4.3.1--1%20%7C%20dev-7b4fb0"></a>
  <img alt="Rust 1.96" src="https://img.shields.io/badge/rust-1.96-e8743b?logo=rust&logoColor=white">
</p>

<p align="center">
  <a href="#installation">Installation</a> ·
  <a href="#using-slot-mini">Usage</a> ·
  <a href="#labels-shells-and-the-cart-studio">Cart studio</a> ·
  <a href="#building-from-source">Building</a> ·
  <a href="#documentation">Docs</a> ·
  <a href="#credits">Credits</a>
</p>

**slot-mini** is a game launcher for the Miyoo Mini Plus running [Onion OS](https://github.com/OnionUI/Onion).
It brings the look and feel of [slot](https://github.com/BrandonKowalski/slot), Brandon Kowalski's frontend for
the Anbernic RG SP, to Game Boy Advance, Game Boy Color and Game Boy games. Your library becomes a shelf of
cartridges, each in its own shell with its own label, and you play a game by sliding it into the slot.

<p align="center">
  <img src="docs/media/demo.gif" alt="Browsing the shelf, switching systems, hearting a cart, changing a shell colour in the cart studio, opening the menu and inserting a cart" width="640">
  <br>
  <sub>Recorded from the app itself (<code>scripts/record-demo.sh</code>) on a card of homebrew games.</sub>
</p>

## Highlights

- **slot's shelf on a 640x480 handheld.** slot's cartridge faces, shells, labels, chrome, insert and eject
  animations and sounds, redrawn by a software renderer built for the Mini Plus, which has no GPU.
- **Your library as it is.** Games are read from Onion's own `Roms/GBA`, `Roms/GBC` and `Roms/GB` folders.
  You don't need to move or rename anything.
- **Cart studio.** Pick each cart's shell colour, finish and shape on the device. Labels and shells use slot's
  file format, so labels made with slot's [Cart Studio](https://studio.slot-cfw.fyi) work as they are.
- **Onion underneath.** Games run in Onion's RetroArch with its saves, save states, hotkeys, shaders and Game
  Switcher. slot-mini adds slot's in-game menu and a per-game choice of core (mGBA or gpSP).
- **Optional boot launcher.** Replace Onion's main menu with slot-mini and come back to the cart you just played
  after every game. If slot-mini is ever missing or fails, Onion's menu starts instead.
- **Over-the-air updates.** MENU → Check for updates installs new releases over Wi-Fi.

## Installation

**Requirements:** a Miyoo Mini Plus with Onion OS **4.3.1-1** (stable) or Onion's **development build**.
slot-mini detects which one is installed.

1. Download `slot-mini-<version>.zip` from the [latest release](https://github.com/joecrockett5/slot-mini/releases/latest).
2. Unzip it at the root of your SD card, so that `App/slot-mini/` sits beside Onion's other apps. The archive
   contains that folder and nothing else.
3. On the device, open **Apps → slot-mini**.

Each system with an Onion emulator installed gets a shelf. Games in a subfolder one level down are included.

> [!TIP]
> Start in app mode. Once you are happy with slot-mini, make it the boot launcher from its menu
> (see [below](#boot-launcher-optional)). The [device checklist](docs/DEVICE-TESTING.md) walks through every
> feature, and is a good way to try a new version on a spare card.

### Updating

With Wi-Fi on, press **MENU** and choose **Check for updates**. When a newer release is available, the row
becomes **Install update x.y.z**. Press A and slot-mini downloads the release, verifies it, swaps it in and
restarts. Only the app's own files are replaced: settings, favourites, labels and the cache are kept. The
previous files are saved to `App/slot-mini/update/previous/`, so you can undo an update from a computer.

Only numbered releases are offered, never the rolling **Latest main build** pre-release, and slot-mini checks
for updates only when you ask. To follow a fork, set `update_repo = owner/repo` in
`App/slot-mini/data/settings.txt`. You can also unzip a release over `App/slot-mini/` by hand.

### Moving to Onion's development build

1. Update slot-mini first (**MENU → Check for updates**) to 0.5.0 or later.
2. Update Onion with its own updater on the beta channel. The device reboots.
3. Onion's update replaces Onion's own files, so if slot-mini was your boot launcher, Onion's menu is back. Open
   **Apps → slot-mini** once: it restores the boot launcher and tells you so. If you had turned it off, it stays off.

On the development build, GBA games without a core set for them run on gpSP, Onion's new default. Onion copies
each game's mGBA save across the first time it runs there.

### Boot launcher (optional)

Press **MENU** in slot-mini and turn **Boot launcher** on. slot-mini then starts in place of Onion's main menu,
and every game brings you back to the shelf, on the cart you just played.

- Hold **L2** while slot-mini starts, or choose **MENU → Exit to Onion**, to reach Onion's own menu.
- **MENU → Boot launcher: Off** restores Onion's menu.
- If slot-mini is missing, crashes or fails to start, Onion's menu starts instead.

slot-mini saves Onion's original `MainUI` binaries to `.tmp_update/slot-mini/stock/` before it replaces
anything. See [how the boot switch works](#how-the-boot-switch-works).

### Splash screens (optional)

**MENU → Splash screens: slot** shows slot's screens while the device starts and shuts down: the **slot.**
wordmark at boot, and **Powering Down** at power off, whether from Onion's menu, an app or a game.
**Splash screens: Onion** brings back Onion's screens, or your theme's.

The screens are installed as Onion theme overrides (`bootScreen.png`, `Screen_Off.png` and `Screen_Off_Save.png`
in `Saves/CurrentProfile/theme/skin/extra/`), so they work with any theme on both Onion versions. If you had put
overrides there yourself, they are backed up and restored when you switch back. Onion still draws its version
number in the bottom-left corner (and the battery level in the top-right corner at power off), as it does on
every theme. If Tweaks' "Reset theme overrides" removes them, slot-mini reinstalls them the next time it starts.

### Uninstalling

1. If the boot launcher is on, turn it off (**MENU → Boot launcher: Off**).
2. If slot's splash screens are on, turn them off (**MENU → Splash screens: Onion**).
3. Delete `App/slot-mini/`.

If you deleted the app with the boot launcher still on, Onion's menu still starts, because the launcher falls
back when slot-mini is missing. To remove it by hand, copy each `.tmp_update/slot-mini/stock/<name>/MainUI`
back over `.tmp_update/bin/<name>`, then delete `.tmp_update/slot-mini/`. Updating Onion also restores its menu.

`Labels/` is in slot's format, so you can keep it. slot-mini's own settings live in `App/slot-mini/data/` and
are removed with the folder.

## Using slot-mini

### On the shelf

| Input | Action |
| --- | --- |
| Left / Right | Browse the shelf (hold to scroll) |
| Up / Down | Jump to the previous or next letter |
| L1 / R1 | Switch shelves: Favorites, GBA, GBC, GB |
| A (tap) | Insert the cart and resume from its auto save state |
| A (hold) | Insert the cart and start fresh, without loading the state |
| Y | Heart or un-heart the cart (hearted carts get a Favorites shelf) |
| SELECT | Open the cart studio |
| START | GBA carts: open the cart and choose its core (mGBA or gpSP) |
| MENU | Settings: sounds, in-game menu, boot launcher, splash screens, updates, exit to Onion |

Holding A starts the game the way Onion's own **Reset game** does, for that one run. RetroArch still writes its
auto state when you quit.

### In a game

| Input | Action |
| --- | --- |
| MENU (tap) | Pause behind slot's in-game menu: Resume, Save State, Load State, Settings (RetroArch's menu), Game Switcher (Onion's), Exit Game |
| MENU (hold about a second) | Eject: RetroArch writes its auto state and quits to the shelf |
| MENU + any button | Onion's and RetroArch's hotkeys, unchanged (MENU + R2 save state, MENU + L2 load state, MENU + R fast forward, …) |

The menu opens instantly, because a small helper waits beside each game slot-mini launches. If any other button
is pressed with MENU, in any order, slot-mini treats it as a hotkey and stays out of the way. On Onion's
development build, **Game Switcher** opens over the paused game, as Onion's own menu does there. On 4.3.1-1 the
game closes first. Games started from Onion's menu keep Onion's MENU, and **MENU → In-game menu: Off** hands MENU
back to Onion everywhere.

### Cores (GBA)

**START** opens the cart, as in slot: the chip sits in the socket of the core the game runs on. Left and Right
move it between **mGBA** and **gpSP**. A keeps the change and B cancels it. The default is Onion's: mGBA on
4.3.1-1, gpSP on the development build. The choice is stored as Onion's own per-game setting
(`Roms/GBA/.game_config/<game>.cfg`), so Onion's Game List Options shows the same core.

RetroArch keeps saves per core, so when the core changes, slot-mini carries the in-game save across, as Onion's
**Transfer mGBA save** does. A save it would overwrite is kept as `<game>.srm.bak_<time>`, and the new core's old
auto state is set aside the same way, so the game resumes from the save you just carried. Each core has its own
save-state format, so save states stay with their core.

## Labels, shells and the cart studio

slot-mini reads the files slot's [Cart Studio](https://studio.slot-cfw.fyi) writes, at the root of the SD card:

- **`Labels/GBA/<rom name>.png`** (and `Labels/GB/`, `Labels/GBC/`): label art, scaled to cover and cropped from
  the centre. GBA labels are 1.94:1, Game Boy and Game Boy Color labels 1.12:1.
- **`Labels/cart_shell.txt`**: one line per game, `<rom name> = <shape> <hex colour> <finish>`, for example
  `Metroid Fusion (USA) = auto 6a3fa0 solid`. The shape is `auto`, `notched` or `rounded`, and the finish is
  `solid`, `clear` or `glitter`. Lines that can't be read are skipped.

The ROM name is the file name without its extension. A cart without label art gets slot's generated text label.

**On the device**, SELECT opens the cart studio for the selected cart. You can set the shell colour, the finish
and, for Game Boy carts, the shape. Changes are saved to `Labels/cart_shell.txt` in the same format.

**The label printer** can fetch labels from slot's art service. It is **off by default** until the service's
author approves third-party use. While it is off, the printer points you to Cart Studio. To turn it on anyway,
add `label_service = on` to `App/slot-mini/data/settings.txt`. With it on and Wi-Fi up, slot-mini also fetches
labels for unlabelled carts in the background. It never replaces a label file you already have.

<details>
<summary><b>Using Cart Studio with an Onion card</b></summary>

Cart Studio expects a slot card, with games in `Games/<system>/` beside `Labels/`. Onion keeps them in
`Roms/<system>/`, and slot-mini does not need `Games/`. Until Cart Studio can read Onion's layout, use one of
these:

- **On a computer (recommended):** create `Games/GBA`, `Games/GB` and `Games/GBC` holding copies of your ROMs
  (only the file names matter). Run Cart Studio on that folder, then copy the `Labels/` folder it writes to the
  root of your SD card.
- **On the card:** create `Games/GBA` etc. at the SD root with copies of the ROMs you want labels for. Run Cart
  Studio on the card, then delete `Games/`. This needs room for the copies.

</details>

## Customisation

- **Wallpaper:** put `.png` files in `Wallpapers/` at the SD root, and slot-mini picks one at random each time it
  starts (slot's wood otherwise). Any size works: images are scaled to cover and cropped.
- **Theme:** copy `App/slot-mini/theme.txt.example` to `theme.txt`. `appearance = light` selects slot's light look,
  and `footer_ink`, `heart_ink` and `wallpaper_tint` adjust the colours.
- **Sounds:** slot's cartridge insert and eject sounds, under **MENU → Sounds** (off by default). They are played
  with Onion's `ffplay`. Turning them on plays the insert sound as a test, and the menu then shows how the player
  finished. With Onion's logging on, its output also goes to `.tmp_update/logs/slot-mini.log`.

<details>
<summary><b>Files slot-mini writes</b></summary>

| File | Contents |
| --- | --- |
| `App/slot-mini/data/settings.txt` | Sounds, in-game menu, boot launcher, splash screens, label service, clock format |
| `App/slot-mini/data/state.txt` | The shelf and cart you were on |
| `App/slot-mini/data/favorites.txt` | Hearted carts, in slot's format |
| `App/slot-mini/cache/` | Scaled labels, ROM headers, the art index, sound clips |
| `App/slot-mini/*`, `App/slot-mini/update/` | Only when installing an update: the new files, and the replaced ones in `update/previous/` |
| `Labels/cart_shell.txt`, `Labels/<sys>/*.png` | Only when you change a cart in the studio or fetch labels |
| `Roms/recentlist.json` | On each launch, as Onion's MainUI does, so Recents and the Game Switcher work |
| `.tmp_update/slot-mini/stock/`, `.tmp_update/bin/MainUI-*` | Only when the boot launcher is turned on |
| `bootScreen.png`, `Screen_Off.png`, `Screen_Off_Save.png` in `Saves/CurrentProfile/theme/skin/extra/` and `.tmp_update/slot-mini/` | Only when slot's splash screens are turned on: slot's screens, and backups of any you had |

</details>

### How the boot switch works

Onion's runtime loop (`.tmp_update/runtime.sh`) bind-mounts one of `.tmp_update/bin/MainUI-<model>-<clean|expert>`
over `miyoo/app/MainUI` and runs it whenever the main menu is due. Turning the boot launcher on:

1. copies each of those files to `.tmp_update/slot-mini/stock/<name>/MainUI`, flushes it to disk and reads it
   back to check it;
2. only then writes a small shell script beside the original and renames it into place.

If this is interrupted at any point, the card has either Onion's original or the script, with the original
safely saved. The script ([package/shim.sh](package/shim.sh)) runs slot-mini. It starts Onion's saved menu
instead when slot-mini is missing, fails or crashes, when L2 is held, when `.tmp_update/slot-mini/DISABLED`
exists, or when you choose Exit to Onion. It runs under the name `sh`, because Onion's key monitor treats any
process named `MainUI` as its menu. Turning the boot launcher off copies the originals back, and never over a
file that is not slot-mini's script (an Onion update may already have restored its own).

## Building from source

The Rust toolchain is pinned in `rust-toolchain.toml`, and rustup installs it automatically. The device build
also needs `clang`, `lld` and `llvm`.

Create a simulated Onion SD card with a few header-only ROMs, then run slot-mini in a 640x480 window:

```bash
cargo run --features slot-mini/desktop -- --make-fake-sd sdcard
```

```bash
cargo run --features slot-mini/desktop -- --sd sdcard --tmp /tmp
```

Desktop keys: arrows, Z = A, X = B, A = Y, S = X, Q / W = L1 / R1, Backspace = SELECT, Escape = MENU.

Build for the device, then package the release zip into `dist/`. The device build is a static 32-bit ARM
binary for the Cortex-A7, with NEON, linked with `rust-lld`.

```bash
cargo build --release --target armv7-unknown-linux-musleabihf -p slot-mini
```

```bash
scripts/package.sh
```

### Testing

```bash
cargo test --workspace
```

```bash
SLOT_MINI_BIN=target/debug/slot-mini sh package/tests/test-shim.sh
```

- **Golden images** of every screen at 640x480 (`crates/slot-mini/tests/golden/`). `UPDATE_GOLDEN=1` rewrites
  them, and [docs/DESIGN-PARITY.md](docs/DESIGN-PARITY.md) shows them beside slot's own.
- **Whole launch cycles** against a stand-in for Onion's runtime loop (`crates/slot-mini/tests/fake-runtime.sh`).
- **The label service** against a local HTTP server, never the internet.
- **Device backends** (framebuffer, input, audio, battery) tested off the device. The whole suite also runs on
  armv7 under `qemu-arm-static` in CI.
- **The boot shim** and its install and restore switch (`package/tests/test-shim.sh`).

To run the armv7 suite locally:

```bash
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER=qemu-arm-static cargo test --release --target armv7-unknown-linux-musleabihf --workspace
```

The demo and banner in this README are made from the app itself. `scripts/record-demo.sh` drives slot-mini
headlessly and encodes `docs/media/demo.gif` (it needs `ffmpeg`), and `scripts/banner.py` draws
`docs/media/banner.png` from the recording (it needs Pillow).

### Releases

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) builds and tests every push and pull request on the
desktop, and runs the full suite on armv7 under qemu. On `main`, the armv7 job also builds the release binary,
tests it and packages the zip. Once everything passes, the `release` job
([`scripts/release.sh`](scripts/release.sh)) publishes it:

- if `v<version>` from the workspace `Cargo.toml` has not been released yet, it creates a release with that tag;
- otherwise it refreshes the rolling `main-latest` pre-release.

To ship a new version, bump `version` in `Cargo.toml` and merge to `main`.

## Documentation

| Document | Contents |
| --- | --- |
| [PLATFORM.md](docs/PLATFORM.md) | What slot-mini relies on in Onion and the hardware, checked against Onion's and Open MainUI's source |
| [DESIGN-PARITY.md](docs/DESIGN-PARITY.md) | Every slot screen and effect, how slot-mini reproduces it, and side-by-side images |
| [DECISIONS.md](docs/DECISIONS.md) | Design decisions and the reasons for them |
| [PROGRESS.md](docs/PROGRESS.md) | Changes by version, performance figures and open questions |
| [DEVICE-TESTING.md](docs/DEVICE-TESTING.md) | A step-by-step checklist for a real Mini Plus |

## Roadmap

Possible next steps, roughly in order of effort:

- A **Recents** shelf (slot has one; Onion's Game Switcher covers it for now).
- Re-tinting **logo labels** after the studio is closed (slot keeps the logo so it can re-tint it).
- The label printer's **print and tear** animation, and the studio cart's apply animation.
- The **sticker curl** on the menu screen.
- **Per-shelf wallpapers** (`GB.png`, `GBA.png`, … as in slot).

Not planned: other Onion systems, a core picker for Game Boy carts, and features that belong to RetroArch or to
slot's own frontend, such as save-state browsing, rewind, shaders, bezels, colour grading, link play, slot's web
companion and RomM.

## Credits

slot-mini exists because of **[slot](https://github.com/BrandonKowalski/slot)** by
**[Brandon Kowalski](https://github.com/BrandonKowalski)**. The shelf, cartridge faces and shells, slot's chrome
and footer, the cart studio, the label service client, ROM scanning, the label and shell file formats, the boot
and shutdown screens and the insert and eject sounds are slot's design, code and assets, adapted for a 640x480
screen without a GPU. Every file copied from slot names its source file and the slot commit it came from
(`e66715d`), and describes what changed. If you have an RG SP, use slot.

- **Art:** slot's skeuomorphic artwork is by **Markus** ([MarkusMaribu](https://github.com/MarkusMaribu)).
- **Fonts:** [Open Sans](https://fonts.google.com/specimen/Open+Sans) (SIL Open Font License 1.1) and
  [Symbols Nerd Font Mono](https://github.com/ryanoasis/nerd-fonts) (MIT), as shipped with slot.
- **Platform:** [Onion OS](https://github.com/OnionUI/Onion) and
  [Open MainUI](https://github.com/robcodedev/onionos-mainui-opensource). Their source documents the runtime
  loop, launch commands, key map, Recents format and how to replace MainUI safely. No code from either is included.
- **Demo:** the demo card uses the titles of freely available homebrew games (Anguna, Apotris, Butano Fighter,
  Celeste Classic, Goodboy Galaxy, Varooom 3D, Dangan GB, Infinity, Shock Lobster, Tobu Tobu Girl, Deadeus,
  Libbet and the Magic Floor, Petris). The ROMs are empty headers, and the Apotris label is slot's example
  label art.
- **Rust crates:** listed with their licences in `App/slot-mini/licenses/THIRD-PARTY.txt` in each release.

slot-mini is an independent project. It is not affiliated with or endorsed by slot, Onion OS or Miyoo.

## Licence

slot-mini is released under the **GNU General Public License v3.0 or later** ([LICENSE](LICENSE)), the licence
of the slot code it is built from. (slot's README badge says MIT, but its workspace is GPL-3.0-or-later.) Every
release ships slot's licence as `licenses/slot-GPL-3.0.txt`, together with the font licences and the licence of
every Rust crate.

## AI disclosure

slot-mini was written by Claude, Anthropic's AI model, working from a written brief: the research into Onion,
the Rust code, the shell scripts, the tests and this documentation. The maintainer tests it on a Miyoo Mini
Plus. slot's own frontend was also built with Claude, as slot's
[AI page](https://github.com/BrandonKowalski/slot/blob/main/site/docs/ai.md) explains. slot's art is Markus's,
and its design is Brandon Kowalski's.

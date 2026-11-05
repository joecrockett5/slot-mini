# slot-mini

A game launcher for the Miyoo Mini Plus on [Onion OS](https://github.com/OnionUI/Onion) that recreates
the look and feel of [slot](https://github.com/BrandonKowalski/slot) by Brandon Kowalski, for GBA, GB and
GBC games: a shelf of cartridges in their own shells and labels, slid into a slot to play.

![The shelf](docs/parity/shelf.png)

*Left: slot on the RG SP. Right: slot-mini, rendered at the Mini Plus's 640x480.*

slot-mini is a launcher. When a game starts, slot-mini exits and Onion's RetroArch takes over, with its
own menu, save states, fast-forward and shaders. Most of what you see is slot's own code and art, copied
and drawn in software (the Mini Plus has no GPU).

> **Not yet tested on a real Mini Plus.** Everything here was built and tested against Onion's source
> and a simulated card. Follow [docs/DEVICE-TESTING.md](docs/DEVICE-TESTING.md) on a spare card before
> trusting it with your main one, and start in app mode.

Not affiliated with slot, Onion or Miyoo.

## Install

1. Download `slot-mini-<version>.zip` from the
   [latest release](https://github.com/joecrockett5/slot-mini/releases/latest). Every push to `main` also
   refreshes the **Latest main build** pre-release, for trying changes before the next version.
2. Unzip it at the root of your Onion SD card, so that `App/slot-mini/` lands next to Onion's other apps.
   The zip contains that folder and nothing else.
3. On the device: **Apps -> slot-mini**.

slot-mini reads your games where Onion keeps them, `Roms/GBA`, `Roms/GBC` and `Roms/GB` (and one level of
subfolders), for every system whose Onion emulator is installed. You never need to move games.

### Use it as the boot launcher (optional)

In slot-mini press **MENU** and turn **Boot launcher** on. From then on slot-mini starts where Onion's
main menu would, and you come straight back to the cart you just played after each game.

- Hold **L2** while slot-mini starts, or pick **MENU -> Exit to Onion**, to get Onion's own menu.
- **MENU -> Boot launcher: Off** puts Onion's menu back.
- If slot-mini is missing, crashes or fails to start, Onion's menu starts instead.

Turning it on replaces Onion's `MainUI` binaries in `.tmp_update/bin/` with a small shell script, after
saving each original to `.tmp_update/slot-mini/stock/`. See [how the switch works](#how-the-boot-switch-works).

## Uninstall

1. If you turned the boot launcher on, turn it off first (**MENU -> Boot launcher: Off**).
2. Delete `App/slot-mini/`.

If you deleted the app before turning the boot launcher off, Onion's menu still starts (the shim falls back
when slot-mini is missing). To remove the shim by hand, with the card in a computer, copy each
`.tmp_update/slot-mini/stock/<name>/MainUI` back over `.tmp_update/bin/<name>`, then delete
`.tmp_update/slot-mini/`. An Onion update also puts Onion's own menu back.

`Labels/` and anything in it is in slot's format and can stay; slot-mini's own settings live inside
`App/slot-mini/data/` and go with the folder.

## Controls

| Input | Action |
| --- | --- |
| Left / Right | Browse the shelf (hold to scroll) |
| Up / Down | Jump to the previous or next letter |
| L1 / R1 | Switch shelves: Favorites, GBA, GBC, GB |
| Tap A | Insert the cart and resume its auto save state |
| Hold A | Insert the cart and start fresh, without loading the state |
| Y | Heart or un-heart the cart (hearted carts get a Favorites shelf) |
| SELECT | Cart studio for this cart |
| MENU | About: sounds, in-game menu, boot launcher, exit to Onion |

Hold A starts the game the way Onion's own "Reset game" (Game List Options) does, for that one run. As with
Onion's Reset game, RetroArch still writes its auto state when you quit the game.

**In a game** started from slot-mini, MENU is slot's:

| Input | Action |
| --- | --- |
| Tap MENU | The game freezes behind slot's in-game menu: Resume, Save State, Load State, Settings (RetroArch's menu), Game Switcher (Onion's), Exit Game |
| Hold MENU | Eject: RetroArch saves its auto state and quits, back to the shelf |

It opens straight away: a small helper is already waiting beside the game, instead of Onion starting its Game Switcher. MENU combinations with other buttons are still Onion's. Games started from Onion's own menu keep Onion's MENU. To give MENU back to Onion in slot-mini's games too, turn **MENU -> In-game menu** off.

In the cart studio: Up / Down pick a row, A opens it, Left / Right change a finish or shape, B goes back and
saves.

## Labels, shells and the cart studio

slot-mini reads exactly the files slot's [Cart Studio](https://studio.slot-cfw.fyi) writes, at the root of
the SD card:

- `Labels/GBA/<rom name>.png`, `Labels/GB/...`, `Labels/GBC/...`: label art, scaled to cover and cropped
  from the centre (GBA labels are 1.94:1, GB and GBC 1.12:1).
- `Labels/cart_shell.txt`: one line per game, `<rom name> = <shape> <hex colour> <finish>`, for example
  `Metroid Fusion (USA) = auto 6a3fa0 solid`. Shape is `auto`, `notched` or `rounded`; finish is `solid`,
  `clear` or `glitter`. Lines that cannot be read are skipped.

The ROM name is the file name without its extension. A cart with no label gets slot's generated text label.

**On the device**, SELECT opens the cart studio: shell colour, finish and (for Game Boy carts) shape, written
back to `Labels/cart_shell.txt` in the same format, and a label printer.

**The label printer** can fetch labels from slot's art service, but that is **off by default** until the
service's author approves third-party use. With it off, the printer tells you to make labels in Cart
Studio on a computer. To turn it on anyway, add `label_service = on` to `App/slot-mini/data/settings.txt`.
With it on and Wi-Fi up, slot-mini also fetches labels for carts that have none, in the background. A label
file you already have is never replaced.

### Cart Studio and the `Games/` folder (open question)

Cart Studio expects a slot card, with games in `Games/<system>/` beside `Labels/`. Onion keeps them in
`Roms/<system>/`, and slot-mini does not need `Games/` at all. Until Cart Studio can be pointed at Onion's
layout, either:

- **On a computer (recommended):** make a folder with `Games/GBA`, `Games/GB`, `Games/GBC` holding copies of
  your ROMs (the file names are what matter), run Cart Studio on it, then copy the `Labels/` folder it
  writes to the root of your SD card; or
- **On the card:** create `Games/GBA` etc. at the SD root with copies of the ROMs you want labels for, run
  Cart Studio on the card, then delete `Games/`. This needs the space for the copies.

## Look

- **Wallpaper:** put `.png` files in `Wallpapers/` at the SD root; slot-mini picks one at random each start
  (slot's own wood otherwise). Any size; it is scaled to cover and cropped.
- **Theme:** copy `App/slot-mini/theme.txt.example` to `theme.txt` and edit: `appearance = light` for slot's
  light look, plus `footer_ink`, `heart_ink` and `wallpaper_tint`.
- **Sounds:** slot's cartridge insert and eject sounds, **MENU -> Sounds**. Off by default: they are played
  through Onion's `ffplay`, which has not been checked on a device yet.

## Files slot-mini writes

| File | What |
| --- | --- |
| `App/slot-mini/data/settings.txt` | Sounds, label service, clock format |
| `App/slot-mini/data/state.txt` | The shelf and cart you were on |
| `App/slot-mini/data/favorites.txt` | Hearted carts, in slot's format |
| `App/slot-mini/cache/` | Scaled labels, ROM headers, the art index, sound clips |
| `Labels/cart_shell.txt`, `Labels/<sys>/*.png` | Only when you change a cart in the studio or fetch labels |
| `Roms/recentlist.json` | Each launch, as Onion's MainUI does, so the Game Switcher and resume work |
| `.tmp_update/slot-mini/stock/`, `.tmp_update/bin/MainUI-*` | Only when you turn the boot launcher on |

## How the boot switch works

Onion's runtime loop (`.tmp_update/runtime.sh`) bind-mounts one of `.tmp_update/bin/MainUI-<model>-<clean|expert>`
over `miyoo/app/MainUI` and runs it whenever the launcher is due. Turning the boot launcher on:

1. copies each of those files that exists to `.tmp_update/slot-mini/stock/<name>/MainUI`, flushes it, and
   reads it back;
2. only then writes the shim beside the original and renames it over.

Stopping at any point leaves either Onion's original or the shim with its original saved. The shim
([package/shim.sh](package/shim.sh)) runs slot-mini and starts the saved original instead when slot-mini is
missing, exits with an error or crashes, L2 is held, `.tmp_update/slot-mini/DISABLED` exists, or you pick
Exit to Onion. It runs under the name `sh`, because Onion's key monitor treats anything named `MainUI` as
its menu and takes over MENU, SELECT, Y and L1. Turning the boot launcher off copies the originals back over
the shims, and never over a file that is not the shim (an Onion update may have put its own back).

## Building

You need the Rust toolchain in `rust-toolchain.toml` (rustup installs it), plus `clang`, `lld` and `llvm` for
the device build.

```bash
cargo run --features slot-mini/desktop -- --make-fake-sd sdcard
```

```bash
cargo run --features slot-mini/desktop -- --sd sdcard --tmp /tmp
```

The first command writes a fake Onion card with a few header-only ROMs; the second opens a 640x480 window.
Keys: arrows, Z = A, X = B, A = Y, S = X, Q / W = L1 / R1, Backspace = SELECT, Escape = MENU.

```bash
cargo build --release --target armv7-unknown-linux-musleabihf -p slot-mini
```

```bash
scripts/package.sh
```

The device build is a static 32-bit ARM binary (Cortex-A7, NEON) linked with `rust-lld`;
`scripts/package.sh` makes the release zip in `dist/`.

### Releases

CI publishes releases from `main` once both builds and all tests pass (`release` job in
`.github/workflows/ci.yml`, script `scripts/release.sh`):

- if `v<version>` from `Cargo.toml` has not been released, it becomes a release tagged at that commit;
- otherwise the push replaces the rolling `main-latest` pre-release.

To cut a new numbered release, bump `version` in the workspace `Cargo.toml` and merge to `main`.

## Testing

```bash
cargo test --workspace
```

```bash
SLOT_MINI_BIN=target/debug/slot-mini sh package/tests/test-shim.sh
```

- Golden images of every screen at 640x480 (`crates/slot-mini/tests/golden/`); `UPDATE_GOLDEN=1` rewrites
  them. [docs/DESIGN-PARITY.md](docs/DESIGN-PARITY.md) puts them next to slot's.
- A stand-in for Onion's runtime loop (`crates/slot-mini/tests/fake-runtime.sh`) runs whole launch cycles.
- The label service is tested against a local HTTP server, never the internet.
- The framebuffer and input backends are tested off-device, and the whole suite also runs on armv7 under
  `qemu-arm-static` in CI.
- `package/tests/test-shim.sh` covers the boot shim and the install and restore switch.

Run the armv7 tests locally with
`CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER=qemu-arm-static cargo test --release --target armv7-unknown-linux-musleabihf --workspace`.

## Documentation

- [docs/PLATFORM.md](docs/PLATFORM.md): what slot-mini relies on in Onion and the hardware, checked
  against Onion's and Open MainUI's source.
- [docs/DESIGN-PARITY.md](docs/DESIGN-PARITY.md): every slot screen and effect, and what slot-mini does
  with it, with side-by-side images.
- [docs/DECISIONS.md](docs/DECISIONS.md): calls made along the way, and why.
- [docs/PROGRESS.md](docs/PROGRESS.md): what was built, performance numbers, open questions.
- [docs/DEVICE-TESTING.md](docs/DEVICE-TESTING.md): the checklist for a real Mini Plus.

## Roadmap

Out of scope for now. The first few would be straightforward follow-ups:

- **Recents shelf** (slot has one; Onion's Game Switcher covers it today).
- **Custom label colours for logo labels** after the studio is closed (slot keeps the logo to re-tint it).
- **The label printer's print and tear animation**, and the studio cart's apply animation.
- **The sticker curl** on the About screen.
- **Per-shelf wallpapers** (`GB.png`, `GBA.png`, ... in slot).
- Battery charging bolt (needs a cheap charging read on the Mini Plus).
- Other Onion systems, the core picker, in-game features (save-state switcher, rewind and fast-forward
  bindings, shaders, bezels, colour grading), link play, the web companion and RomM belong to RetroArch or
  slot's own frontend and are not planned.

## Credits

- **[slot](https://github.com/BrandonKowalski/slot)** by Brandon Kowalski: the shelf, cartridge faces and
  shells, slot chrome, footer, cart studio, label service client, ROM scanning, the label and shell file
  formats, and the insert and eject sounds are slot's code and assets. Each copied file says where it came
  from and what changed.
- **Markus** ([MarkusMaribu](https://github.com/MarkusMaribu)): slot's skeuomorphic art.
- **Fonts:** Open Sans (SIL Open Font License 1.1) and Symbols Nerd Font Mono (MIT), as shipped with slot.
- **[Onion OS](https://github.com/OnionUI/Onion)** and **[Open MainUI](https://github.com/robcodedev/onionos-mainui-opensource)**:
  their source is how slot-mini knows the runtime loop, launch command, key map, Recents format and how to
  replace MainUI safely. No code from either is included.
- Rust crates: see `App/slot-mini/licenses/THIRD-PARTY.txt` in the release.

## Licence

GPL-3.0-or-later ([LICENSE](LICENSE)), because slot is: slot's workspace is GPL-3.0-or-later (its README
badge says MIT, but the code is GPL). slot's licence ships in the release as `licenses/slot-GPL-3.0.txt`, with
the font licences and every Rust crate's licence.

## AI disclosure

slot-mini was written by Claude, Anthropic's AI model, working from a written brief: the research into Onion,
the Rust code, the shell scripts, the tests and these documents. slot's own frontend was also put together
with Claude, as slot's [AI page](https://github.com/BrandonKowalski/slot/blob/main/site/docs/ai.md) explains;
the art is Markus's and slot's design is Brandon Kowalski's. Nothing here has been run on hardware yet, which
is what [docs/DEVICE-TESTING.md](docs/DEVICE-TESTING.md) is for.

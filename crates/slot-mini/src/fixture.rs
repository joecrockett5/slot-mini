//! A fake Onion SD card for tests and the desktop build: emulator folders,
//! ROMs with real header fields (so slot's shell tables pick colours), a
//! couple of labels and a cart_shell.txt line.
//!
//! The ROMs are headers padded with zeros, not games.

use std::path::Path;

use slot_mini_store::Platform;

/// (platform, file name, header title, header code, GB CGB flag)
pub const GAMES: &[(Platform, &str, &str, &str, u8)] = &[
    (
        Platform::Gba,
        "Advance Wars (USA).gba",
        "ADVANCEWARS",
        "AWRE",
        0,
    ),
    (
        Platform::Gba,
        "Golden Sun (USA, Europe).gba",
        "Golden_Sun_A",
        "AGSE",
        0,
    ),
    (
        Platform::Gba,
        "Metroid Fusion (USA).gba",
        "METROID4USA",
        "AMTE",
        0,
    ),
    (
        Platform::Gba,
        "Pokemon - Emerald Version (USA, Europe).gba",
        "POKEMON EMER",
        "BPEE",
        0,
    ),
    (
        Platform::Gba,
        "Pokemon - Ruby Version (USA).gba",
        "POKEMON RUBY",
        "AXVE",
        0,
    ),
    (
        Platform::Gba,
        "Mario Kart - Super Circuit (USA).gba",
        "MARIOKARTSC",
        "AMKE",
        0,
    ),
    (
        Platform::Gba,
        "2 Games in 1 (Europe).gba",
        "2IN1",
        "BLAP",
        0,
    ),
    (
        Platform::Gbc,
        "Pokemon - Crystal Version (USA, Europe).gbc",
        "PM_CRYSTAL",
        "BYTE",
        0xC0,
    ),
    (
        Platform::Gbc,
        "Legend of Zelda, The - Oracle of Ages (USA).gbc",
        "ZELDA DIN",
        "AZ8E",
        0xC0,
    ),
    (
        Platform::Gbc,
        "Wario Land 3 (World).gbc",
        "WARIOLAND3",
        "AW8E",
        0xC0,
    ),
    (Platform::Gb, "Tetris (World).gb", "TETRIS", "", 0),
    (
        Platform::Gb,
        "Kirby's Dream Land (USA, Europe).gb",
        "KIRBY DREAM",
        "",
        0,
    ),
    (
        Platform::Gb,
        "Pokemon - Yellow Version (USA, Europe).gb",
        "POKEMON YELLOW",
        "",
        0x80,
    ),
    (
        Platform::Gb,
        "Super Mario Land (World).gb",
        "SUPER MARIOLAND",
        "",
        0,
    ),
];

fn rom_bytes(platform: Platform, title: &str, code: &str, cgb: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    match platform {
        Platform::Gba | Platform::Gbav => {
            let t = &title.as_bytes()[..title.len().min(12)];
            rom[0xa0..0xa0 + t.len()].copy_from_slice(t);
            rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
            rom[0xb2] = 0x96;
        }
        Platform::Gb | Platform::Gbc => {
            let t = &title.as_bytes()[..title.len().min(11)];
            rom[0x134..0x134 + t.len()].copy_from_slice(t);
            if code.len() == 4 {
                rom[0x13f..0x143].copy_from_slice(code.as_bytes());
            }
            rom[0x143] = cgb;
            rom[0x14a] = 1; // overseas
        }
    }
    rom
}

/// Onion's emulator launch script, cut down: enough for the runtime's
/// checks (it mentions `.retroarch/cores`).
pub fn launch_script(core: &str) -> String {
    format!(
        "#!/bin/sh\necho $0 $*\nprogdir=`dirname \"$0\"`\ncd /mnt/SDCARD/RetroArch/\nHOME=/mnt/SDCARD/RetroArch/ ./retroarch -v -L .retroarch/cores/{core}_libretro.so \"$1\"\n"
    )
}

/// Write the fake card at `sd`. `labels` adds slot's example label art for
/// two GBA games and a cart_shell.txt line.
pub fn build(sd: &Path, labels: bool) -> std::io::Result<()> {
    for (p, core) in [
        (Platform::Gba, "mgba"),
        (Platform::Gb, "gambatte"),
        (Platform::Gbc, "gambatte"),
    ] {
        let emu = sd.join("Emu").join(p.dir_name());
        std::fs::create_dir_all(&emu)?;
        std::fs::write(emu.join("launch.sh"), launch_script(core))?;
        std::fs::write(
            emu.join("config.json"),
            format!(
                "{{\n\"label\":\"{0}\",\n\"launch\":\"launch.sh\",\n\"rompath\":\"../../Roms/{0}\",\n\"imgpath\":\"../../Roms/{0}/Imgs\"\n}}\n",
                p.dir_name()
            ),
        )?;
        let roms = sd.join("Roms").join(p.dir_name());
        std::fs::create_dir_all(roms.join("Imgs"))?;
    }
    for (p, name, title, code, cgb) in GAMES {
        let path = sd.join("Roms").join(p.dir_name()).join(name);
        std::fs::write(path, rom_bytes(*p, title, code, *cgb))?;
    }
    // RetroArch's cores, as placeholders: Hold A reads which one a system
    // uses, as Onion's Game List Options does.
    let cores = sd.join("RetroArch/.retroarch/cores");
    std::fs::create_dir_all(&cores)?;
    for core in ["mgba_libretro", "gpsp_libretro", "gambatte_libretro"] {
        std::fs::write(cores.join(format!("{core}.so")), b"not a real core")?;
    }
    std::fs::create_dir_all(sd.join(".tmp_update/config"))?;
    std::fs::write(
        sd.join("system.json"),
        "{\n\t\"vol\":\t20,\n\t\"wifi\":\t0\n}\n",
    )?;
    if labels {
        let dir = sd.join("Labels/GBA");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("Pokemon - Emerald Version (USA, Europe).png"),
            EXAMPLE_REAL,
        )?;
        std::fs::write(dir.join("Golden Sun (USA, Europe).png"), EXAMPLE_LOGO)?;
        std::fs::write(
            sd.join("Labels/cart_shell.txt"),
            "Tetris (World) = notched 2f5cc0 glitter\nAdvance Wars (USA) = auto 3a7d44 clear\nbroken line\n",
        )?;
    }
    Ok(())
}

const EXAMPLE_REAL: &[u8] = include_bytes!("../../../assets/fetch_example_real.png");
const EXAMPLE_LOGO: &[u8] = include_bytes!("../../../assets/fetch_example_logo.png");

//! The memory budget while ROMs cross: the real binary, paired with a hub
//! on localhost that holds the largest GBA games there are, runs MENU,
//! Sync now and reports its peak resident memory on the way out.

mod common;

use std::process::Command;

use common::desk::{rom, write, Desk};
use common::Rig;

const BIN: &str = env!("CARGO_BIN_EXE_slot-mini");
/// PROMPT.md's budget for slot-mini's resident memory.
const BUDGET_KB: u64 = 40 * 1024;

/// Under qemu the figure is the emulator's, not slot-mini's.
#[test]
#[cfg_attr(target_arch = "arm", ignore)]
fn a_rom_transfer_stays_inside_the_memory_budget() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    desk.pair_and_seed(&rig);
    // Two games as large as a GBA cart gets, and some ordinary ones.
    for n in 0..2u8 {
        let at = desk.root.join(format!("Roms/GBA/Huge {n}.gba"));
        write(&at, &rom(32 << 20, n));
    }
    for n in 0..4u8 {
        let at = desk.root.join(format!("Roms/GBA/Plain {n}.gba"));
        write(&at, &rom(8 << 20, 16 + n));
    }
    std::fs::write(
        rig.paths.data().join("settings.txt"),
        "online_labels = off\nauto_sync = off\n",
    )
    .unwrap();
    // MENU, down to Sync now, A; the frames left over are the wait.
    let input = "800:press:MENU,816:release:MENU,1700:press:DOWN,1716:release:DOWN,\
                 1900:press:A,1916:release:A";
    let out = Command::new(BIN)
        .arg("--sd")
        .arg(&rig.paths.sd)
        .arg("--tmp")
        .arg(&rig.paths.tmp)
        .args(["--mode", "launcher", "--backend", "headless", "--sim"])
        .args(["--frames", "4000", "--input", input])
        .env("SLOT_MINI_SIM_WIFI", "1")
        .env(
            "SLOT_MINI_SYNC_DISCOVER",
            desk.options().discover[0].to_string(),
        )
        .env("SLOT_MINI_REPORT_RSS", "1")
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    for n in 0..2u8 {
        let at = rig.paths.sd.join(format!("Roms/GBA/Huge {n}.gba"));
        let len = std::fs::metadata(&at).map(|m| m.len()).unwrap_or(0);
        assert_eq!(len, 32 << 20, "{} crossed whole\n{log}", at.display());
    }
    let peak: u64 = log
        .lines()
        .find_map(|l| l.strip_prefix("slot-mini: VmHWM:"))
        .and_then(|l| l.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| panic!("no peak reported\n{log}"));
    println!("peak resident memory with 96 MB of ROMs crossing: {peak} kB");
    assert!(peak < BUDGET_KB, "peak {peak} kB, budget {BUDGET_KB} kB");
}

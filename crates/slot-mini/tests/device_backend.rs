//! The device backend end to end, off the device: the real binary draws
//! through the framebuffer path into a file laid out like /dev/fb0, and the
//! frame there (turned back the right way up) matches the headless render.

use std::process::Command;

mod common;

const BIN: &str = env!("CARGO_BIN_EXE_slot-mini");

fn runner() -> Vec<String> {
    match cfg!(target_arch = "arm") {
        true => std::env::var("CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER")
            .ok()
            .filter(|r| !r.is_empty())
            .into_iter()
            .collect(),
        false => Vec::new(),
    }
}

fn run(rig: &common::Rig, args: &[&str], env: &[(&str, &str)]) {
    let mut r = runner();
    let program = match r.is_empty() {
        true => BIN.to_string(),
        false => r.remove(0),
    };
    let mut cmd = Command::new(program);
    if !runner().is_empty() {
        cmd.arg(BIN);
    }
    cmd.args([
        "--sd",
        rig.paths.sd.to_str().unwrap(),
        "--tmp",
        rig.paths.tmp.to_str().unwrap(),
    ])
    .args(["--sim", "--frames", "120"])
    .args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    assert!(cmd.status().unwrap().success());
}

#[test]
fn the_framebuffer_gets_the_same_frame_upside_down() {
    let rig = common::Rig::new();
    let fb = rig.dir.path().join("fb0");
    std::fs::write(&fb, vec![0u8; 640 * 960 * 4]).unwrap();
    let shot = rig.dir.path().join("shot.png");
    run(
        &rig,
        &["--backend", "device", "--shot", shot.to_str().unwrap()],
        &[
            ("SLOT_MINI_FB", fb.to_str().unwrap()),
            ("SLOT_MINI_INPUT", "/dev/null"),
        ],
    );
    let bytes = std::fs::read(&fb).unwrap();
    assert_eq!(
        bytes.len(),
        640 * 960 * 4,
        "the file keeps the framebuffer's size"
    );
    // On exit the last frame is handed back on page 0, where Onion's own
    // programs (the in-game Game Switcher) expect the screen to be.
    let decoder = png::Decoder::new(std::fs::File::open(&shot).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut want = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut want).unwrap();
    let page = |p: usize| -> Vec<u8> {
        let mut rgba = Vec::with_capacity(640 * 480 * 4);
        // Turn the page back: pixel i of the frame is at N-1-i.
        let base = p * 640 * 480 * 4;
        for i in (0..640 * 480).rev() {
            let px = &bytes[base + i * 4..base + i * 4 + 4];
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        rgba
    };
    assert!(page(0) == want, "page 0 does not hold the last frame");
}

//! Over-the-air updates against a local stand-in for GitHub: the release
//! API, a release zip built here, and a "new binary" that is a shell script
//! answering `--version`.

mod common;
#[path = "common/server.rs"]
mod server;

use std::collections::HashMap;

use common::Rig;
use server::Server;
use slot_mini::update::{self, Fail, Status};
use slot_mini::{Exit, Phase};
use slot_mini_gfx::Button;

/// A zip as `zip -r` writes it: deflated entries with Unix modes.
fn zip(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dir = Vec::new();
    for (name, bytes, mode) in files {
        let packed = miniz_oxide::deflate::compress_to_vec(bytes, 6);
        let crc = crc32fast::hash(bytes);
        let at = out.len() as u32;
        let header = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend((3u16 << 8 | 30).to_le_bytes()); // made by Unix
            }
            h.extend(20u16.to_le_bytes()); // version needed
            h.extend(0u16.to_le_bytes()); // flags
            h.extend(8u16.to_le_bytes()); // deflate
            h.extend(0u32.to_le_bytes()); // time, date
            h.extend(crc.to_le_bytes());
            h.extend((packed.len() as u32).to_le_bytes());
            h.extend((bytes.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes()); // extra
            if central {
                h.extend(0u16.to_le_bytes()); // comment
                h.extend(0u16.to_le_bytes()); // disk
                h.extend(0u16.to_le_bytes()); // internal attributes
                h.extend((mode << 16).to_le_bytes());
                h.extend(at.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(header(0x0403_4b50, false));
        out.extend(&packed);
        dir.extend(header(0x0201_4b50, true));
    }
    let dir_at = out.len() as u32;
    out.extend(&dir);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((dir.len() as u32).to_le_bytes());
    out.extend(dir_at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

fn script(version: &str) -> Vec<u8> {
    format!("#!/bin/sh\necho \"slot-mini {version}\"\n").into_bytes()
}

fn release_zip(version: &str) -> Vec<u8> {
    let bin = script(version);
    zip(&[
        ("App/", b"", 0o40755),
        ("App/slot-mini/slot-mini", &bin, 0o100755),
        (
            "App/slot-mini/launch.sh",
            b"#!/bin/sh\necho new\n",
            0o100755,
        ),
        ("App/slot-mini/icon.png", b"new icon", 0o100644),
        ("App/slot-mini/licenses/NEW.txt", b"new licence", 0o100644),
        // Never taken from a zip.
        (
            "App/slot-mini/data/settings.txt",
            b"sounds = on\n",
            0o100644,
        ),
        ("App/slot-mini/../../evil", b"x", 0o100644),
        ("Other/file", b"x", 0o100644),
    ])
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// GitHub as the updater sees it: `latest` (the release API answer) and the
/// zip it points at, on a second server (the download URL needs an address).
struct GitHub {
    api: Server,
    _assets: Server,
}

impl std::ops::Deref for GitHub {
    type Target = Server;
    fn deref(&self) -> &Server {
        &self.api
    }
}

fn github(version: &str, zip: Vec<u8>, digest: &str) -> GitHub {
    let size = zip.len();
    let mut files = HashMap::new();
    files.insert(format!("slot-mini-{version}.zip"), zip);
    let assets = Server::start(files);
    let json = format!(
        r#"{{"tag_name":"v{version}","draft":false,"prerelease":false,"assets":[
            {{"name":"slot-mini-{version}.zip","size":{size},"digest":"sha256:{digest}",
              "browser_download_url":"{}/slot-mini-{version}.zip"}}]}}"#,
        assets.base
    );
    let mut files = HashMap::new();
    files.insert("latest".to_string(), json.into_bytes());
    GitHub {
        api: Server::start(files),
        _assets: assets,
    }
}

/// A card with the current version of the app's files.
fn card(rig: &Rig) {
    let app = rig.paths.app();
    std::fs::create_dir_all(rig.paths.data()).unwrap();
    std::fs::write(app.join("slot-mini"), script("0.0.1")).unwrap();
    std::fs::write(app.join("launch.sh"), "#!/bin/sh\necho old\n").unwrap();
    std::fs::write(app.join("icon.png"), "old icon").unwrap();
    std::fs::write(rig.paths.data().join("settings.txt"), "sounds = off\n").unwrap();
}

#[test]
fn a_newer_release_is_found_and_an_older_one_is_not() {
    let z = release_zip("9.9.9");
    let s = github("9.9.9", z.clone(), &sha256_hex(&z));
    let url = format!("{}/latest", s.base);
    let r = update::check_against(&url, "0.2.2").unwrap().unwrap();
    assert_eq!(r.version, "9.9.9");
    assert_eq!(r.size, z.len() as u64);
    assert_eq!(update::check_against(&url, "9.9.9").unwrap(), None);
    assert_eq!(update::check_against(&url, "10.0.0").unwrap(), None);
    let missing = format!("{}/nothing", s.base);
    assert_eq!(
        update::check_against(&missing, "0.1.0"),
        Err(Fail::NoRelease)
    );
}

#[test]
fn an_update_replaces_the_app_files_and_nothing_else() {
    let rig = Rig::new();
    card(&rig);
    let z = release_zip("9.9.9");
    let s = github("9.9.9", z.clone(), &sha256_hex(&z));
    let r = update::check_against(&format!("{}/latest", s.base), "0.0.1")
        .unwrap()
        .unwrap();
    let mut seen = Vec::new();
    let bytes = update::download(&r, |got, total| seen.push((got, total))).unwrap();
    assert_eq!(seen.last(), Some(&(z.len() as u64, z.len() as u64)));
    let files = update::unzip(&bytes).unwrap();
    let mut names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        ["icon.png", "launch.sh", "licenses/NEW.txt", "slot-mini"]
    );
    update::install(&rig.paths, &files, "9.9.9").unwrap();

    let app = rig.paths.app();
    let read = |p: &str| std::fs::read_to_string(app.join(p)).unwrap();
    assert_eq!(read("slot-mini"), "#!/bin/sh\necho \"slot-mini 9.9.9\"\n");
    assert_eq!(read("icon.png"), "new icon");
    assert_eq!(read("licenses/NEW.txt"), "new licence");
    assert_eq!(read("data/settings.txt"), "sounds = off\n");
    assert!(!rig.paths.sd.join("evil").exists());
    // The old files are kept.
    assert_eq!(read("update/previous/icon.png"), "old icon");
    assert!(read("update/previous/slot-mini").contains("0.0.1"));
    assert!(!app.join("update/new").exists());
    // The restarted binary notices, once.
    assert_eq!(update::after_restart(&rig.paths).as_deref(), Some("9.9.9"));
    assert_eq!(update::after_restart(&rig.paths), None);
}

#[test]
fn a_damaged_download_is_refused() {
    let z = release_zip("9.9.9");
    let s = github("9.9.9", z.clone(), &"0".repeat(64));
    let r = update::check_against(&format!("{}/latest", s.base), "0.0.1")
        .unwrap()
        .unwrap();
    assert!(matches!(
        update::download(&r, |_, _| {}),
        Err(Fail::Corrupt(_))
    ));
    // A flipped byte inside an entry fails its CRC.
    let mut bad = z.clone();
    let name = b"App/slot-mini/slot-mini";
    let data = bad.windows(name.len()).position(|w| w == name).unwrap() + name.len();
    bad[data + 2] ^= 0xff;
    assert!(matches!(update::unzip(&bad), Err(Fail::Corrupt(_))));
}

#[test]
fn a_binary_that_is_not_the_promised_version_changes_nothing() {
    let rig = Rig::new();
    card(&rig);
    let files = update::unzip(&release_zip("9.9.8")).unwrap();
    let e = update::install(&rig.paths, &files, "9.9.9").unwrap_err();
    assert!(matches!(e, Fail::Broken(_)), "{e:?}");
    let app = rig.paths.app();
    assert!(std::fs::read_to_string(app.join("slot-mini"))
        .unwrap()
        .contains("0.0.1"));
    assert_eq!(
        std::fs::read_to_string(app.join("icon.png")).unwrap(),
        "old icon"
    );
    assert_eq!(update::after_restart(&rig.paths), None);
}

fn wait_for(d: &mut common::Driver, what: impl Fn(&Status) -> bool) -> Status {
    d.wait_until("the updater", |d| what(&d.app.updater.status()));
    d.app.updater.status()
}

#[test]
fn menu_check_install_and_restart() {
    let rig = Rig::new();
    card(&rig);
    let z = release_zip("9.9.9");
    let s = github("9.9.9", z.clone(), &sha256_hex(&z));
    std::fs::write(
        rig.paths.data().join("settings.txt"),
        format!("update_repo = {}/latest\n", s.base),
    )
    .unwrap();
    let mut d = rig.app();
    d.tap(Button::Menu);
    assert_eq!(d.app.phase, Phase::About);
    // "Check for updates" sits just above "Exit to Onion", the last row.
    d.tap(Button::Up);
    d.tap(Button::Up);
    d.tap(Button::A);
    let found = wait_for(&mut d, |s| !matches!(s, Status::Checking));
    assert!(
        matches!(&found, Status::Available(r) if r.version == "9.9.9"),
        "{found:?}"
    );
    d.run(650);
    common::golden("about_update", &d.frame);
    d.tap(Button::A);
    wait_for(&mut d, |s| matches!(s, Status::Installed(_)));
    // B is ignored while an install finishes; the restart follows.
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::About);
    d.run(1600);
    assert_eq!(d.app.exit, Some(Exit::Restart));
    assert!(std::fs::read_to_string(rig.paths.app().join("slot-mini"))
        .unwrap()
        .contains("9.9.9"));
}

#[test]
fn being_up_to_date_is_said() {
    let rig = Rig::new();
    card(&rig);
    let z = release_zip("0.0.1");
    let s = github("0.0.1", z.clone(), &sha256_hex(&z));
    std::fs::write(
        rig.paths.data().join("settings.txt"),
        format!("update_repo = {}/latest\n", s.base),
    )
    .unwrap();
    let mut d = rig.app();
    d.tap(Button::Menu);
    d.tap(Button::Up);
    d.tap(Button::Up);
    d.tap(Button::A);
    assert_eq!(
        wait_for(&mut d, |s| !matches!(s, Status::Checking)),
        Status::UpToDate
    );
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::Shelf);
}

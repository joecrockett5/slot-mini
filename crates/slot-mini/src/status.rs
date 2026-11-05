//! The footer's battery, Wi-Fi and clock, read the cheap way Onion exposes
//! them (see docs/PLATFORM.md). A fixed source is used for tests.

use std::path::PathBuf;

use crate::paths::Paths;

pub trait Status {
    /// Battery percentage, if known.
    fn battery(&mut self) -> Option<u8>;
    fn wifi(&mut self) -> bool;
    /// Local wall-clock time as (hour, minute).
    fn clock(&mut self) -> (u32, u32);
}

/// Fixed values, for tests and screenshots.
pub struct FixedStatus {
    pub battery: Option<u8>,
    pub wifi: bool,
    pub clock: (u32, u32),
}

impl Status for FixedStatus {
    fn battery(&mut self) -> Option<u8> {
        self.battery
    }
    fn wifi(&mut self) -> bool {
        self.wifi
    }
    fn clock(&mut self) -> (u32, u32) {
        self.clock
    }
}

/// Onion's files: `/tmp/percBat`, `system.json`'s `wifi`, `wlan0`'s state.
pub struct OnionStatus {
    perc_bat: PathBuf,
    system_json: PathBuf,
    operstate: PathBuf,
}

impl OnionStatus {
    pub fn new(paths: &Paths) -> OnionStatus {
        OnionStatus {
            perc_bat: paths.tmp.join("percBat"),
            system_json: paths.sd.join("system.json"),
            operstate: PathBuf::from("/sys/class/net/wlan0/operstate"),
        }
    }

    #[cfg(test)]
    fn with_operstate(mut self, p: PathBuf) -> Self {
        self.operstate = p;
        self
    }
}

/// batmon's file: a decimal percentage. Over 100 is a known bad read.
pub fn parse_perc_bat(text: &str) -> Option<u8> {
    let n: i64 = text.trim().parse().ok()?;
    (0..=100).contains(&n).then_some(n as u8)
}

/// `"wifi": 1` in MainUI's system.json, without a JSON parser.
pub fn wifi_setting(json: &str) -> bool {
    let Some(at) = json.find("\"wifi\"") else {
        return false;
    };
    let rest = json[at + 6..]
        .trim_start()
        .trim_start_matches(':')
        .trim_start();
    rest.starts_with('1')
}

impl Status for OnionStatus {
    fn battery(&mut self) -> Option<u8> {
        parse_perc_bat(&std::fs::read_to_string(&self.perc_bat).ok()?)
    }

    fn wifi(&mut self) -> bool {
        let on = std::fs::read_to_string(&self.system_json).is_ok_and(|j| wifi_setting(&j));
        on && std::fs::read_to_string(&self.operstate).is_ok_and(|s| s.trim() == "up")
    }

    fn clock(&mut self) -> (u32, u32) {
        local_hm()
    }
}

/// Hours and minutes in the local zone (TZ from the environment).
pub fn local_hm() -> (u32, u32) {
    // SAFETY: localtime_r writes only into `tm`.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return (0, 0);
        }
        (tm.tm_hour as u32, tm.tm_min as u32)
    }
}

pub fn clock_text((h, m): (u32, u32), h24: bool) -> String {
    match h24 {
        true => format!("{h:02}:{m:02}"),
        false => format!("{}:{m:02}", if h % 12 == 0 { 12 } else { h % 12 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perc_bat() {
        assert_eq!(parse_perc_bat("87\n"), Some(87));
        assert_eq!(parse_perc_bat("1735289191"), None);
        assert_eq!(parse_perc_bat(""), None);
    }

    #[test]
    fn wifi_from_system_json() {
        assert!(wifi_setting("{\n\t\"vol\":\t20,\n\t\"wifi\":\t1,\n}"));
        assert!(!wifi_setting("{\"wifi\": 0}"));
        assert!(!wifi_setting("{}"));
    }

    #[test]
    fn onion_files() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        std::fs::create_dir_all(&paths.sd).unwrap();
        std::fs::create_dir_all(&paths.tmp).unwrap();
        let oper = d.path().join("operstate");
        let mut s = OnionStatus::new(&paths).with_operstate(oper.clone());
        assert_eq!(s.battery(), None);
        assert!(!s.wifi());
        std::fs::write(paths.tmp.join("percBat"), "42").unwrap();
        std::fs::write(paths.sd.join("system.json"), "{\"wifi\":\t1}").unwrap();
        std::fs::write(&oper, "up\n").unwrap();
        assert_eq!(s.battery(), Some(42));
        assert!(s.wifi());
    }

    #[test]
    fn clock_formats() {
        assert_eq!(clock_text((9, 5), true), "09:05");
        assert_eq!(clock_text((0, 30), false), "12:30");
        assert_eq!(clock_text((13, 0), false), "1:00");
    }
}

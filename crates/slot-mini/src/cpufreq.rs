//! The CPU's frequency governor, and slot-mini's own CPU use, for an idle
//! launcher that should let the CPU slow down (docs/PLATFORM.md, Power).
//!
//! Onion's runtime puts the governor back to `ondemand` after every game
//! (`launch_game_postprocess`). Onion's `cpuclock` tool, games and the
//! scrapers set `userspace` or `performance` meanwhile; as the boot launcher
//! slot-mini can start before anything has set it back (at boot, or after a
//! tool), and with `performance` the CPU never slows down while the shelf
//! sits idle. [`settle`] does what the runtime does.

use std::path::{Path, PathBuf};

/// Where the runtime writes the governor.
pub const CPUFREQ: &str = "/sys/devices/system/cpu/cpu0/cpufreq";

/// Governors that keep the CPU from slowing down on an idle screen.
const PINNED: [&str; 2] = ["performance", "userspace"];

fn read(dir: &Path, name: &str) -> Option<String> {
    let s = std::fs::read_to_string(dir.join(name)).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// If the governor is one that pins the clock, set `ondemand` (as Onion's
/// runtime does after a game) and return the governor it replaced.
pub fn settle(dir: &Path) -> Option<String> {
    let governor = read(dir, "scaling_governor")?;
    if !PINNED.contains(&governor.as_str()) {
        return None;
    }
    let available = read(dir, "scaling_available_governors").unwrap_or_default();
    if !available.is_empty() && !available.split_whitespace().any(|g| g == "ondemand") {
        return None;
    }
    std::fs::write(dir.join("scaling_governor"), "ondemand").ok()?;
    Some(governor)
}

/// The governor and current clock, e.g. "ondemand 408 MHz".
pub fn describe(dir: &Path) -> Option<String> {
    let governor = read(dir, "scaling_governor")?;
    Some(
        match read(dir, "scaling_cur_freq").and_then(|f| f.parse::<u64>().ok()) {
            Some(khz) => format!("{governor} {} MHz", khz / 1000),
            None => governor,
        },
    )
}

/// This process's CPU use, as a share of one core between two calls to
/// [`CpuUse::sample`].
pub struct CpuUse {
    stat: PathBuf,
    last: Option<(u64, std::time::Instant)>,
}

impl CpuUse {
    pub fn new() -> CpuUse {
        CpuUse {
            stat: PathBuf::from("/proc/self/stat"),
            last: None,
        }
    }

    /// utime + stime, in clock ticks.
    fn ticks(&self) -> Option<u64> {
        let s = std::fs::read_to_string(&self.stat).ok()?;
        // The command name can hold spaces; fields resume after its ')'.
        let rest = &s[s.rfind(')')? + 1..];
        let f: Vec<&str> = rest.split_whitespace().collect();
        // After ')': state is field 3, utime 14 and stime 15.
        Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
    }

    /// Percent of one core used since the last sample (None the first time).
    pub fn sample(&mut self) -> Option<f32> {
        let now = std::time::Instant::now();
        let ticks = self.ticks()?;
        let previous = self.last.replace((ticks, now));
        let (t0, at) = previous?;
        let secs = now.duration_since(at).as_secs_f32();
        // SAFETY: sysconf has no side effects.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f32;
        (secs > 0.0).then(|| (ticks.saturating_sub(t0) as f32 / hz) / secs * 100.0)
    }
}

impl Default for CpuUse {
    fn default() -> Self {
        CpuUse::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(governor: &str, available: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("scaling_governor"), format!("{governor}\n")).unwrap();
        std::fs::write(d.path().join("scaling_available_governors"), available).unwrap();
        std::fs::write(d.path().join("scaling_cur_freq"), "408000\n").unwrap();
        d
    }

    #[test]
    fn a_pinned_governor_goes_back_to_ondemand() {
        for g in ["performance", "userspace"] {
            let d = dir(g, "ondemand userspace powersave performance\n");
            assert_eq!(settle(d.path()).as_deref(), Some(g));
            assert_eq!(
                read(d.path(), "scaling_governor").as_deref(),
                Some("ondemand")
            );
        }
    }

    #[test]
    fn other_governors_are_left_alone() {
        for g in ["ondemand", "powersave", "conservative", "interactive"] {
            let d = dir(g, "ondemand userspace powersave performance\n");
            assert_eq!(settle(d.path()), None);
            assert_eq!(read(d.path(), "scaling_governor").as_deref(), Some(g));
        }
        // No ondemand on this kernel: nothing is written.
        let d = dir("performance", "userspace performance\n");
        assert_eq!(settle(d.path()), None);
        assert_eq!(
            read(d.path(), "scaling_governor").as_deref(),
            Some("performance")
        );
        // No cpufreq at all.
        assert_eq!(settle(Path::new("/nonexistent")), None);
    }

    #[test]
    fn the_clock_is_described() {
        let d = dir("ondemand", "");
        assert_eq!(describe(d.path()).as_deref(), Some("ondemand 408 MHz"));
    }

    #[test]
    fn own_cpu_use_is_sampled() {
        let mut c = CpuUse::new();
        assert_eq!(c.sample(), None, "the first sample has nothing to compare");
        let t = std::time::Instant::now();
        while t.elapsed().as_millis() < 50 {
            std::hint::black_box((0..1000).sum::<u64>());
        }
        let p = c.sample().unwrap();
        assert!((0.0..=400.0).contains(&p), "{p}");
    }
}

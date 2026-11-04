//! What the harness knows about the machine: load, throttling, and the fingerprint.
//!
//! Everything that reads the system sits behind [`SystemProbe`], so the guard logic is tested
//! with injected load. Probes shell out to `sysctl`, `pmset` and `uname` or read `/proc` and
//! `/sys`; anything that cannot be read is `None`, never a guess.

use std::process::Command;

use serde::{Deserialize, Serialize};

/// Throttling signals that are cheap to read. All fields are `None` where the OS offers nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Throttle {
    /// macOS `pmset -g therm` `CPU_Speed_Limit` (percent; 100 means unthrottled).
    pub speed_limit_pct: Option<u32>,
    /// Linux `thermal_throttle/core_throttle_count` of cpu0 (a counter: a rise means throttling).
    pub throttle_count: Option<u64>,
}

/// Source of machine state for the guard. Implement it to inject load in tests.
pub trait SystemProbe: Send + Sync {
    /// 1-minute load average.
    fn load1(&self) -> Option<f64>;
    fn logical_cores(&self) -> usize;
    fn throttle(&self) -> Throttle;
}

/// Reads the real machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealProbe;

impl SystemProbe for RealProbe {
    fn load1(&self) -> Option<f64> {
        if let Ok(s) = std::fs::read_to_string("/proc/loadavg") {
            return parse_proc_loadavg(&s);
        }
        parse_sysctl_loadavg(&cmd("sysctl", &["-n", "vm.loadavg"])?)
    }

    fn logical_cores(&self) -> usize {
        std::thread::available_parallelism().map_or(1, usize::from)
    }

    fn throttle(&self) -> Throttle {
        let speed_limit_pct = cmd("pmset", &["-g", "therm"]).and_then(|s| parse_pmset_speed_limit(&s));
        let throttle_count =
            std::fs::read_to_string("/sys/devices/system/cpu/cpu0/thermal_throttle/core_throttle_count").ok().and_then(|s| s.trim().parse().ok());
        Throttle { speed_limit_pct, throttle_count }
    }
}

fn cmd(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `0.52 0.58 0.59 1/467 12345` -> 0.52
pub fn parse_proc_loadavg(s: &str) -> Option<f64> {
    s.split_whitespace().next()?.parse().ok()
}

/// `{ 2.19 2.35 2.37 }` -> 2.19
pub fn parse_sysctl_loadavg(s: &str) -> Option<f64> {
    s.trim().trim_start_matches('{').split_whitespace().next()?.parse().ok()
}

/// Finds `CPU_Speed_Limit = 80` in `pmset -g therm` output.
pub fn parse_pmset_speed_limit(s: &str) -> Option<u32> {
    let line = s.lines().find(|l| l.contains("CPU_Speed_Limit"))?;
    line.split('=').nth(1)?.trim().parse().ok()
}

/// The quiet-machine verdict, stored in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guard {
    /// True when the numbers should not be published.
    pub contended: bool,
    /// Why, one line per cause. Empty when clean.
    pub reasons: Vec<String>,
    pub load_before: Option<f64>,
    pub load_after: Option<f64>,
    pub logical_cores: usize,
    /// Load above `max_load_per_core * logical_cores` is contended.
    pub max_load_per_core: f64,
    pub load_threshold: f64,
    pub throttle_before: Throttle,
    pub throttle_after: Throttle,
}

/// Raw readings for [`evaluate`].
#[derive(Debug, Clone)]
pub struct Readings {
    pub load_before: Option<f64>,
    pub load_after: Option<f64>,
    pub logical_cores: usize,
    pub max_load_per_core: f64,
    pub throttle_before: Throttle,
    pub throttle_after: Throttle,
}

/// Applies the guard rules. Unknown load is recorded as a reason-free `None`, not as
/// contention: only a reading above the threshold refuses.
pub fn evaluate(r: Readings) -> Guard {
    let threshold = r.max_load_per_core * r.logical_cores as f64;
    let mut reasons = Vec::new();
    for (when, load) in [("before", r.load_before), ("after", r.load_after)] {
        if let Some(l) = load.filter(|l| *l > threshold) {
            reasons.push(format!(
                "1-min load average {l:.2} {when} the run exceeds {threshold:.2} ({} x {} cores)",
                r.max_load_per_core, r.logical_cores
            ));
        }
    }
    for (when, t) in [("before", &r.throttle_before), ("after", &r.throttle_after)] {
        if let Some(p) = t.speed_limit_pct.filter(|p| *p < 100) {
            reasons.push(format!("CPU speed limit {p}% {when} the run (thermal throttling)"));
        }
    }
    if let (Some(b), Some(a)) = (r.throttle_before.throttle_count, r.throttle_after.throttle_count)
        && a > b
    {
        reasons.push(format!("thermal throttle counter rose from {b} to {a} during the run"));
    }
    Guard {
        contended: !reasons.is_empty(),
        reasons,
        load_before: r.load_before,
        load_after: r.load_after,
        logical_cores: r.logical_cores,
        max_load_per_core: r.max_load_per_core,
        load_threshold: threshold,
        throttle_before: r.throttle_before,
        throttle_after: r.throttle_after,
    }
}

/// Where the numbers were taken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub os: String,
    pub arch: String,
    pub kernel: Option<String>,
    pub cpu_model: Option<String>,
    pub logical_cores: usize,
    pub physical_cores: Option<usize>,
    pub ram_bytes: Option<u64>,
    /// `rustc -V` of the compiler that built the harness.
    pub rustc: String,
    pub profile: String,
    pub opt_level: String,
    pub debug_assertions: bool,
    /// RFC 3339, UTC.
    pub date: String,
}

impl Fingerprint {
    pub fn collect(logical_cores: usize) -> Self {
        let linux_cpu = std::fs::read_to_string("/proc/cpuinfo").ok();
        let cpu_model = cmd("sysctl", &["-n", "machdep.cpu.brand_string"]).or_else(|| linux_cpu.as_deref().and_then(parse_cpuinfo_model));
        let physical_cores =
            cmd("sysctl", &["-n", "hw.physicalcpu"]).and_then(|s| s.parse().ok()).or_else(|| linux_cpu.as_deref().and_then(parse_cpuinfo_physical));
        let ram_bytes = cmd("sysctl", &["-n", "hw.memsize"])
            .and_then(|s| s.parse().ok())
            .or_else(|| std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| parse_meminfo_total(&s)));
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let date = chrono::DateTime::from_timestamp(i64::try_from(secs).unwrap_or(0), 0).map(|d| d.to_rfc3339()).unwrap_or_default();
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            kernel: cmd("uname", &["-sr"]),
            cpu_model,
            logical_cores,
            physical_cores,
            ram_bytes,
            rustc: env!("CRAFT_BENCH_RUSTC").to_string(),
            profile: env!("CRAFT_BENCH_PROFILE").to_string(),
            opt_level: env!("CRAFT_BENCH_OPT_LEVEL").to_string(),
            debug_assertions: cfg!(debug_assertions),
            date,
        }
    }
}

/// First `model name` (x86) or `Model` / `Hardware` (ARM) line of `/proc/cpuinfo`.
pub fn parse_cpuinfo_model(s: &str) -> Option<String> {
    s.lines()
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim(), v.trim())))
        .find(|(k, _)| matches!(*k, "model name" | "Model" | "Hardware"))
        .map(|(_, v)| v.to_string())
}

/// Distinct (physical id, core id) pairs in `/proc/cpuinfo`.
pub fn parse_cpuinfo_physical(s: &str) -> Option<usize> {
    let mut pairs = std::collections::BTreeSet::new();
    for block in s.split("\n\n") {
        let field = |name: &str| block.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == name).map(|(_, v)| v.trim().to_string()));
        if let Some(core) = field("core id") {
            pairs.insert((field("physical id").unwrap_or_default(), core));
        }
    }
    (!pairs.is_empty()).then_some(pairs.len())
}

/// `MemTotal:  16384000 kB` -> bytes.
pub fn parse_meminfo_total(s: &str) -> Option<u64> {
    let line = s.lines().find(|l| l.starts_with("MemTotal:"))?;
    line.split_whitespace().nth(1)?.parse::<u64>().ok()?.checked_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readings(before: Option<f64>, after: Option<f64>, cores: usize) -> Readings {
        Readings {
            load_before: before,
            load_after: after,
            logical_cores: cores,
            max_load_per_core: 0.5,
            throttle_before: Throttle::default(),
            throttle_after: Throttle::default(),
        }
    }

    #[test]
    fn quiet_machine_is_clean() {
        let g = evaluate(readings(Some(1.9), Some(2.0), 4));
        assert!(!g.contended && g.reasons.is_empty());
        assert_eq!(g.load_threshold, 2.0);
        assert_eq!((g.load_before, g.load_after), (Some(1.9), Some(2.0)));
    }

    #[test]
    fn load_above_half_the_cores_is_contended_before_or_after() {
        assert!(evaluate(readings(Some(2.01), Some(0.0), 4)).contended);
        let g = evaluate(readings(Some(0.1), Some(3.0), 4));
        assert!(g.contended);
        assert!(g.reasons[0].contains("after"));
    }

    #[test]
    fn unknown_load_does_not_refuse() {
        let g = evaluate(readings(None, None, 8));
        assert!(!g.contended);
        assert_eq!(g.load_before, None);
    }

    #[test]
    fn throttling_signals_refuse() {
        let mut r = readings(Some(0.0), Some(0.0), 4);
        r.throttle_after.speed_limit_pct = Some(80);
        assert!(evaluate(r).contended);
        let mut r = readings(Some(0.0), Some(0.0), 4);
        r.throttle_before.throttle_count = Some(3);
        r.throttle_after.throttle_count = Some(5);
        assert!(evaluate(r).contended);
        let mut r = readings(Some(0.0), Some(0.0), 4);
        r.throttle_before.throttle_count = Some(3);
        r.throttle_after.throttle_count = Some(3);
        r.throttle_after.speed_limit_pct = Some(100);
        assert!(!evaluate(r).contended);
    }

    #[test]
    fn parsers() {
        assert_eq!(parse_proc_loadavg("0.52 0.58 0.59 1/467 12345\n"), Some(0.52));
        assert_eq!(parse_sysctl_loadavg("{ 2.19 2.35 2.37 }"), Some(2.19));
        assert_eq!(parse_sysctl_loadavg("garbage"), None);
        assert_eq!(parse_pmset_speed_limit("Note: No thermal warning level\n  CPU_Speed_Limit \t= 80\n"), Some(80));
        assert_eq!(parse_pmset_speed_limit("No thermal warning level has been recorded\n"), None);
        assert_eq!(parse_meminfo_total("MemTotal:       16384000 kB\nMemFree: 1 kB\n"), Some(16_384_000 * 1024));
        let info = "processor : 0\nmodel name\t: Intel(R) Xeon(R)\nphysical id : 0\ncore id : 0\n\nprocessor : 1\nmodel name : Intel(R) Xeon(R)\nphysical id : 0\ncore id : 0\n\nprocessor : 2\nphysical id : 0\ncore id : 1\n";
        assert_eq!(parse_cpuinfo_model(info).as_deref(), Some("Intel(R) Xeon(R)"));
        assert_eq!(parse_cpuinfo_physical(info), Some(2));
    }

    #[test]
    fn fingerprint_has_the_fields() {
        let f = Fingerprint::collect(4);
        assert!(!f.os.is_empty() && !f.rustc.is_empty() && !f.date.is_empty());
        assert_eq!(f.logical_cores, 4);
    }
}

//! CPU, RAM and GPU utilization for the expanded island.
//!
//! Sampled only while the island is expanded, so an idle island costs nothing.

use serde::Serialize;

use crate::usage::{level, Ring, Thresholds};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SystemView {
    pub cpu: Option<Ring>,
    pub ram: Option<Ring>,
    pub gpu: Option<Ring>,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
}

fn ring(pct: f64, t: Thresholds) -> Ring {
    let pct = (pct.clamp(0.0, 100.0) * 10.0).round() / 10.0;
    Ring { pct, resets_at: None, level: level(pct, t) }
}

pub struct Sampler {
    sys: sysinfo::System,
    gpu: gpu::Gpu,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        let mut sys = sysinfo::System::new();
        // CPU usage is a delta: prime the first sample.
        sys.refresh_cpu_usage();
        Self { sys, gpu: gpu::Gpu::new() }
    }

    pub fn sample(&mut self, t: Thresholds) -> SystemView {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        const GB: f64 = 1024.0 * 1024.0 * 1024.0;
        SystemView {
            cpu: Some(ring(self.sys.global_cpu_usage() as f64, t)),
            ram: (total > 0).then(|| ring(used as f64 / total as f64 * 100.0, t)),
            gpu: self.gpu.sample().map(|p| ring(p, t)),
            ram_used_gb: used as f64 / GB,
            ram_total_gb: total as f64 / GB,
        }
    }
}

/// GPU utilization like Task Manager: per engine type, sum the utilization
/// of every instance, then take the busiest engine type.
pub fn gpu_percent(items: &[(String, f64)]) -> Option<f64> {
    if items.is_empty() {
        return None;
    }
    let mut by_type: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for (name, v) in items {
        let engtype = name.rsplit_once("engtype_").map(|(_, t)| t).unwrap_or("other");
        *by_type.entry(engtype).or_default() += v.max(0.0);
    }
    by_type.values().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v)))).map(|v| v.min(100.0))
}

#[cfg(windows)]
mod gpu {
    use windows_sys::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    };

    /// PDH query on `\GPU Engine(*)\Utilization Percentage`.
    pub struct Gpu {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
        ok: bool,
    }

    // The handles are only used from the sampling task.
    unsafe impl Send for Gpu {}

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    impl Gpu {
        pub fn new() -> Self {
            // SAFETY: out-pointers are valid locals; failures leave ok = false.
            unsafe {
                let mut query: PDH_HQUERY = std::mem::zeroed();
                let mut counter: PDH_HCOUNTER = std::mem::zeroed();
                let mut ok = PdhOpenQueryW(std::ptr::null(), 0, &mut query) == 0;
                if ok {
                    let path = wide(r"\GPU Engine(*)\Utilization Percentage");
                    ok = PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) == 0;
                    // Rate counters need a first collection before values exist.
                    ok = ok && PdhCollectQueryData(query) == 0;
                }
                Self { query, counter, ok }
            }
        }

        pub fn sample(&mut self) -> Option<f64> {
            if !self.ok {
                return None;
            }
            // SAFETY: buffer is sized by the first call and outlives the reads.
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return None;
                }
                let (mut size, mut count) = (0u32, 0u32);
                let r = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
                if r != PDH_MORE_DATA || size == 0 {
                    return None;
                }
                let item = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = vec![std::mem::zeroed(); (size as usize).div_ceil(item)];
                if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, buf.as_mut_ptr()) != 0 {
                    return None;
                }
                let items: Vec<(String, f64)> = buf[..count as usize]
                    .iter()
                    .filter(|i| i.FmtValue.CStatus == 0)
                    .map(|i| {
                        let mut len = 0;
                        while *i.szName.add(len) != 0 {
                            len += 1;
                        }
                        let name = String::from_utf16_lossy(std::slice::from_raw_parts(i.szName, len));
                        (name, i.FmtValue.Anonymous.doubleValue)
                    })
                    .collect();
                super::gpu_percent(&items)
            }
        }
    }

    impl Drop for Gpu {
        fn drop(&mut self) {
            if self.ok {
                // SAFETY: query was opened by PdhOpenQueryW.
                unsafe {
                    PdhCloseQuery(self.query);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod gpu {
    use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType};
    use objc2_io_kit::{
        io_iterator_t, io_object_t, IOIteratorNext, IOObjectRelease, IORegistryEntryCreateCFProperty,
        IOServiceGetMatchingServices, IOServiceMatching,
    };

    /// `Device Utilization %` from the `PerformanceStatistics` of every
    /// IOAccelerator (the value Activity Monitor's GPU history shows), taking
    /// the busiest GPU. Works for Apple silicon and Intel/AMD GPUs.
    pub struct Gpu {
        stats_key: CFRetained<CFString>,
        util_key: CFRetained<CFString>,
    }

    // CFStrings are immutable and only used from the sampling task.
    unsafe impl Send for Gpu {}

    impl Gpu {
        pub fn new() -> Self {
            Self {
                stats_key: CFString::from_static_str("PerformanceStatistics"),
                util_key: CFString::from_static_str("Device Utilization %"),
            }
        }

        pub fn sample(&mut self) -> Option<f64> {
            let mut iter: io_iterator_t = 0;
            // SAFETY: the matching dictionary is consumed by
            // IOServiceGetMatchingServices; iter and every entry are released.
            // Port 0 is the default main port (kIOMainPortDefault is macOS 12+).
            unsafe {
                let matching = IOServiceMatching(c"IOAccelerator".as_ptr())?;
                let matching = CFRetained::cast_unchecked::<CFDictionary>(matching);
                if IOServiceGetMatchingServices(0, Some(matching), &mut iter) != 0 || iter == 0 {
                    return None;
                }
                let mut best: Option<f64> = None;
                loop {
                    let entry = IOIteratorNext(iter);
                    if entry == 0 {
                        break;
                    }
                    if let Some(v) = self.utilization(entry) {
                        best = Some(best.map_or(v, |b| b.max(v)));
                    }
                    IOObjectRelease(entry);
                }
                IOObjectRelease(iter);
                best.map(|v| v.clamp(0.0, 100.0))
            }
        }

        /// # Safety
        /// `entry` must be a live registry entry.
        unsafe fn utilization(&self, entry: io_object_t) -> Option<f64> {
            let stats = unsafe { IORegistryEntryCreateCFProperty(entry, Some(&self.stats_key), None, 0)? };
            let stats = stats.downcast::<CFDictionary>().ok()?;
            // SAFETY: PerformanceStatistics is a dictionary keyed by strings.
            let stats = unsafe { CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(stats) };
            let n = stats.get(&self.util_key)?.downcast::<CFNumber>().ok()?;
            n.as_f64().or_else(|| n.as_i64().map(|v| v as f64))
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod gpu {
    /// Not implemented on this platform; the ring shows "--".
    pub struct Gpu;

    impl Gpu {
        pub fn new() -> Self {
            Gpu
        }

        pub fn sample(&mut self) -> Option<f64> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_takes_busiest_engine_type() {
        let items = vec![
            ("pid_1_luid_0x1_phys_0_eng_0_engtype_3D".to_string(), 20.0),
            ("pid_2_luid_0x1_phys_0_eng_0_engtype_3D".to_string(), 15.0),
            ("pid_2_luid_0x1_phys_0_eng_5_engtype_VideoDecode".to_string(), 30.0),
        ];
        assert_eq!(gpu_percent(&items), Some(35.0));
        assert_eq!(gpu_percent(&[]), None);
        assert_eq!(gpu_percent(&[("x_engtype_3D".into(), 250.0)]), Some(100.0));
    }

    /// Machine-dependent: `cargo test -p clawed gpu_live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn gpu_live() {
        let mut s = Sampler::new();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let v = s.sample(Thresholds::default());
        println!("cpu {:?} ram {:?} gpu {:?}", v.cpu.map(|r| r.pct), v.ram.map(|r| r.pct), v.gpu.map(|r| r.pct));
    }

    #[test]
    fn sampler_reports_cpu_and_ram() {
        let mut s = Sampler::new();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let v = s.sample(Thresholds::default());
        assert!(v.cpu.is_some());
        assert!(v.ram.as_ref().is_some_and(|r| r.pct > 0.0));
        assert!(v.ram_total_gb > 0.0);
    }
}

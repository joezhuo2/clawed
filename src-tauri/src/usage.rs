//! Plan usage rings: exact data from the account usage endpoint or the status
//! line, local estimate otherwise.

use islet_proto::{StatusLine, Window};
use serde::Serialize;

use crate::tailer::EstimateTotals;

/// Status line data older than this falls back to the estimate.
pub const EXACT_STALE_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ring {
    pub pct: f64,
    /// Unix epoch seconds.
    pub resets_at: Option<u64>,
    pub level: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageView {
    pub five_hour: Option<Ring>,
    pub seven_day: Option<Ring>,
    /// `exact`, `estimated` or `unknown`.
    pub source: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
pub struct Thresholds {
    pub warn: f64,
    pub crit: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { warn: 70.0, crit: 90.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
pub struct Caps {
    pub five_hour_tokens: u64,
    pub seven_day_tokens: u64,
}

impl Default for Caps {
    fn default() -> Self {
        Self { five_hour_tokens: 20_000_000, seven_day_tokens: 300_000_000 }
    }
}

pub fn level(pct: f64, t: Thresholds) -> &'static str {
    if pct >= t.crit {
        "crit"
    } else if pct >= t.warn {
        "warn"
    } else {
        "ok"
    }
}

fn ring(w: &Window, now_s: u64, t: Thresholds) -> Ring {
    // A window whose reset time has passed has rolled over.
    let rolled = w.resets_at.is_some_and(|r| r <= now_s);
    let pct = if rolled { 0.0 } else { w.used_pct.clamp(0.0, 100.0) };
    Ring { pct, resets_at: if rolled { None } else { w.resets_at }, level: level(pct, t) }
}

#[derive(Debug, Default)]
pub struct Usage {
    exact: Option<(Option<Window>, Option<Window>, u64)>,
    estimate: Option<EstimateTotals>,
}

impl Usage {
    /// Returns true if the status line carried rate limit data.
    pub fn apply_status(&mut self, st: &StatusLine) -> bool {
        if st.five_hour.is_none() && st.seven_day.is_none() {
            return false;
        }
        self.exact = Some((st.five_hour.clone(), st.seven_day.clone(), st.ts));
        true
    }

    /// Exact windows from the account usage endpoint.
    pub fn apply_api(&mut self, five_hour: Option<Window>, seven_day: Option<Window>, now_ms: u64) {
        self.exact = Some((five_hour, seven_day, now_ms));
    }

    pub fn set_estimate(&mut self, totals: EstimateTotals) {
        self.estimate = Some(totals);
    }

    pub fn has_fresh_exact(&self, now_ms: u64) -> bool {
        self.exact.as_ref().is_some_and(|(_, _, ts)| now_ms.saturating_sub(*ts) < EXACT_STALE_MS)
    }

    pub fn view(&self, now_ms: u64, t: Thresholds, caps: Caps) -> UsageView {
        let now_s = now_ms / 1000;
        if let Some((five, seven, _)) = self.exact.as_ref().filter(|_| self.has_fresh_exact(now_ms)) {
            return UsageView {
                five_hour: five.as_ref().map(|w| ring(w, now_s, t)),
                seven_day: seven.as_ref().map(|w| ring(w, now_s, t)),
                source: "exact",
            };
        }
        if let Some(e) = self.estimate {
            // Reset times from the last status line stay valid until they pass.
            let (five, seven) = self.exact.as_ref().map_or((None, None), |(f, s, _)| (f.as_ref(), s.as_ref()));
            let last_reset = |w: Option<&Window>| w.and_then(|w| w.resets_at).filter(|r| *r > now_s);
            let pct = |used: u64, cap: u64| (used as f64 / cap.max(1) as f64 * 100.0).min(100.0);
            let mk = |p: f64, resets_at: Option<u64>| Ring { pct: (p * 10.0).round() / 10.0, resets_at, level: level(p, t) };
            return UsageView {
                five_hour: Some(mk(pct(e.five_hour, caps.five_hour_tokens), last_reset(five))),
                seven_day: Some(mk(pct(e.seven_day, caps.seven_day_tokens), last_reset(seven))),
                source: "estimated",
            };
        }
        UsageView { five_hour: None, seven_day: None, source: "unknown" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(five: Option<f64>, resets: Option<u64>, ts: u64) -> StatusLine {
        StatusLine {
            v: 1,
            session_id: "s".into(),
            context_used_pct: None,
            context_window_size: None,
            five_hour: five.map(|p| Window { used_pct: p, resets_at: resets }),
            seven_day: Some(Window { used_pct: 12.0, resets_at: None }),
            model: None,
            ppid: 1,
            ts,
        }
    }

    const NOW: u64 = 1_790_000_000_000;

    #[test]
    fn unknown_when_nothing() {
        let v = Usage::default().view(NOW, Thresholds::default(), Caps::default());
        assert_eq!(v.source, "unknown");
        assert!(v.five_hour.is_none());
    }

    #[test]
    fn exact_from_status() {
        let mut u = Usage::default();
        assert!(u.apply_status(&status(Some(31.0), Some(NOW / 1000 + 3600), NOW)));
        let v = u.view(NOW, Thresholds::default(), Caps::default());
        assert_eq!(v.source, "exact");
        assert_eq!(v.five_hour.unwrap().pct, 31.0);
        assert_eq!(v.seven_day.unwrap().pct, 12.0);
    }

    #[test]
    fn status_without_limits_ignored() {
        let mut st = status(None, None, NOW);
        st.seven_day = None;
        assert!(!Usage::default().apply_status(&st));
    }

    #[test]
    fn exact_goes_stale() {
        let mut u = Usage::default();
        u.apply_status(&status(Some(31.0), None, NOW));
        u.set_estimate(EstimateTotals { five_hour: 2_000_000, seven_day: 30_000_000 });
        let v = u.view(NOW + EXACT_STALE_MS, Thresholds::default(), Caps::default());
        assert_eq!(v.source, "estimated");
        assert_eq!(v.five_hour.unwrap().pct, 10.0);
        assert_eq!(v.seven_day.unwrap().pct, 10.0);
    }

    #[test]
    fn stale_exact_keeps_future_reset() {
        let mut u = Usage::default();
        let later = NOW + EXACT_STALE_MS;
        u.apply_status(&status(Some(31.0), Some(later / 1000 + 60), NOW));
        u.set_estimate(EstimateTotals { five_hour: 0, seven_day: 0 });
        let v = u.view(later, Thresholds::default(), Caps::default());
        assert_eq!(v.source, "estimated");
        assert_eq!(v.five_hour.unwrap().resets_at, Some(later / 1000 + 60));
        assert_eq!(v.seven_day.unwrap().resets_at, None);
        // Once the reset passes it is dropped.
        let v = u.view(later + 61_000, Thresholds::default(), Caps::default());
        assert_eq!(v.five_hour.unwrap().resets_at, None);
    }

    #[test]
    fn threshold_levels() {
        let t = Thresholds::default();
        assert_eq!(level(69.9, t), "ok");
        assert_eq!(level(70.0, t), "warn");
        assert_eq!(level(95.0, t), "crit");
    }

    #[test]
    fn reset_rollover_zeroes_pct() {
        let mut u = Usage::default();
        u.apply_status(&status(Some(88.0), Some(NOW / 1000 - 1), NOW));
        let r = u.view(NOW, Thresholds::default(), Caps::default()).five_hour.unwrap();
        assert_eq!((r.pct, r.resets_at, r.level), (0.0, None, "ok"));
    }

    #[test]
    fn estimate_capped_at_100() {
        let mut u = Usage::default();
        u.set_estimate(EstimateTotals { five_hour: u64::MAX / 2, seven_day: 0 });
        let r = u.view(NOW, Thresholds::default(), Caps::default()).five_hour.unwrap();
        assert_eq!((r.pct, r.level), (100.0, "crit"));
    }
}

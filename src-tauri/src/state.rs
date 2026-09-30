//! Shared backend state and the snapshot pushed to the island.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use clawed_proto::now_ms;
use serde::Serialize;
use tokio::sync::Notify;

use crate::approvals::{ApprovalView, Approvals};
use crate::settings::Settings;
use crate::store::{SessionView, Store};
use crate::tailer::{ContextTailer, EstimateScanner};
use crate::usage::{Usage, UsageView};

#[derive(Default)]
pub struct Shared {
    pub store: Mutex<Store>,
    pub approvals: Mutex<Approvals>,
    pub usage: Mutex<Usage>,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub tailer: Mutex<ContextTailer>,
    pub estimator: Mutex<EstimateScanner>,
    /// Sessions whose transcript should be re-read for context usage.
    pub context_dirty: Mutex<HashSet<String>>,
    /// Anything the island or tray shows changed.
    pub dirty: Notify,
    /// Last time any session was active or an approval was pending.
    pub last_active: AtomicU64,
    pub autostarted: AtomicBool,
}

/// Locks a mutex, recovering from poisoning (a panicked holder only ever
/// leaves plain data behind).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub sessions: Vec<SessionView>,
    pub approvals: Vec<ApprovalView>,
    pub usage: UsageView,
    pub paused: bool,
    pub now: u64,
}

impl Shared {
    pub fn new(settings_path: PathBuf, settings: Settings) -> Self {
        let paused = settings.pause_approvals;
        let s = Self {
            settings_path,
            settings: Mutex::new(settings),
            last_active: AtomicU64::new(now_ms()),
            ..Default::default()
        };
        lock(&s.approvals).paused = paused;
        s
    }

    pub fn mark_dirty(&self) {
        self.dirty.notify_one();
    }

    pub fn touch_active(&self) {
        self.last_active.store(now_ms(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Snapshot {
        let now = now_ms();
        let (thresholds, caps) = {
            let s = lock(&self.settings);
            (s.thresholds, s.caps)
        };
        let approvals = lock(&self.approvals);
        Snapshot {
            sessions: lock(&self.store).snapshot(now),
            approvals: approvals.views(),
            paused: approvals.paused,
            usage: lock(&self.usage).view(now, thresholds, caps),
            now,
        }
    }

    /// True while something should keep the island alive.
    pub fn is_busy(&self) -> bool {
        lock(&self.store).any_active() || !lock(&self.approvals).is_empty()
    }
}

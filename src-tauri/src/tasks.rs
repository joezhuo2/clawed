//! Background loops: emission, liveness, context, usage, idle teardown.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use clawed_proto::now_ms;
use tauri::{AppHandle, Emitter};

use crate::state::{lock, Shared};
use crate::transcript::{effective_window, window_for_model};

/// Max emission rate to the island (10 Hz).
const EMIT_INTERVAL: Duration = Duration::from_millis(100);
const LIVENESS_INTERVAL: Duration = Duration::from_secs(10);
const CONTEXT_INTERVAL: Duration = Duration::from_secs(2);
const USAGE_BUSY: Duration = Duration::from_secs(60);
const USAGE_IDLE: Duration = Duration::from_secs(300);
/// Minimum gap between account usage requests.
const USAGE_API_MIN_MS: u64 = 120_000;
const IDLE_CHECK: Duration = Duration::from_secs(30);

pub fn spawn_all(app: AppHandle, shared: Arc<Shared>) {
    tauri::async_runtime::spawn(emitter(app.clone(), shared.clone()));
    tauri::async_runtime::spawn(liveness(shared.clone()));
    tauri::async_runtime::spawn(context(shared.clone()));
    tauri::async_runtime::spawn(usage(shared.clone()));
    tauri::async_runtime::spawn(system(app.clone(), shared.clone()));
    tauri::async_runtime::spawn(idle_teardown(app, shared));
}

const SYSTEM_INTERVAL: Duration = Duration::from_millis(1500);

/// Samples CPU/RAM/GPU only while the island is expanded.
async fn system(app: AppHandle, shared: Arc<Shared>) {
    let mut sampler: Option<crate::system::Sampler> = None;
    loop {
        if !shared.expanded.load(Ordering::Relaxed) {
            // Drop the sampler (and its PDH query) while collapsed.
            sampler = None;
            shared.expanded_changed.notified().await;
            continue;
        }
        let s = sampler.get_or_insert_with(crate::system::Sampler::new);
        let thresholds = lock(&shared.settings).thresholds;
        // CPU usage needs an interval between refreshes.
        tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
        let view = s.sample(thresholds);
        let _ = app.emit_to(crate::window::LABEL, "system", &view);
        tokio::select! {
            _ = tokio::time::sleep(SYSTEM_INTERVAL) => {}
            _ = shared.expanded_changed.notified() => {}
        }
    }
}

/// Coalesces `dirty` notifications: at most one snapshot per 100 ms.
async fn emitter(app: AppHandle, shared: Arc<Shared>) {
    loop {
        shared.dirty.notified().await;
        let snap = shared.snapshot();
        if std::env::var_os("CLAWED_DEBUG").is_some() {
            eprintln!("[clawed] state {}", serde_json::to_string(&snap).unwrap_or_default());
        }
        let _ = app.emit_to(crate::window::LABEL, "state", &snap);
        crate::tray::refresh(&app, &shared, &snap);
        tokio::time::sleep(EMIT_INTERVAL).await;
    }
}

/// Marks sessions whose `claude` process died, and drops expired ones.
async fn liveness(shared: Arc<Shared>) {
    loop {
        tokio::time::sleep(LIVENESS_INTERVAL).await;
        let tracked = lock(&shared.store).tracked_pids();
        let dead: Vec<&String> = tracked
            .iter()
            .filter(|(_, pid)| !crate::liveness::is_alive(*pid))
            .map(|(id, _)| id)
            .collect();
        let now = now_ms();
        let mut changed = false;
        {
            let mut store = lock(&shared.store);
            for id in dead {
                changed |= store.mark_stale(id, now);
            }
            changed |= store.sweep(now);
        }
        if shared.is_busy() {
            shared.touch_active();
        }
        if changed {
            shared.mark_dirty();
        }
    }
}

/// Re-reads transcripts of sessions that had events, for context usage.
async fn context(shared: Arc<Shared>) {
    loop {
        tokio::time::sleep(CONTEXT_INTERVAL).await;
        let ids: Vec<String> = lock(&shared.context_dirty).drain().collect();
        if ids.is_empty() {
            continue;
        }
        let overrides = lock(&shared.settings).context_overrides.clone();
        let targets: Vec<(String, PathBuf, Option<String>)> = {
            let store = lock(&shared.store);
            ids.iter()
                .filter_map(|id| {
                    let s = store.sessions.get(id)?;
                    Some((id.clone(), PathBuf::from(s.transcript_path.as_ref()?), s.model.clone()))
                })
                .collect()
        };
        let shared2 = shared.clone();
        let results = tokio::task::spawn_blocking(move || {
            let mut tailer = lock(&shared2.tailer);
            let active: Vec<PathBuf> = {
                let store = lock(&shared2.store);
                store.sessions.values().filter_map(|s| s.transcript_path.as_ref().map(PathBuf::from)).collect()
            };
            tailer.retain(&active);
            targets
                .into_iter()
                .filter_map(|(id, path, model)| {
                    let u = tailer.poll(&path)?;
                    let model = model.or(u.model.clone()).unwrap_or_default();
                    let tokens = u.context_tokens();
                    Some((id, tokens, effective_window(tokens, window_for_model(&model, &overrides))))
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();

        let now = now_ms();
        let mut changed = false;
        {
            let mut store = lock(&shared.store);
            for (id, tokens, window) in results {
                changed |= store.apply_transcript_context(&id, tokens, window, now);
            }
        }
        if changed {
            shared.mark_dirty();
        }
    }
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
}

fn projects_dir() -> Option<PathBuf> {
    Some(config_dir()?.join("projects"))
}

/// Polls the account usage endpoint, falling back to the local estimate when
/// neither it nor the status line has fresh data.
async fn usage(shared: Arc<Shared>) {
    let mut last_api_error = String::new();
    let mut last_api_at = 0;
    loop {
        let due = now_ms().saturating_sub(last_api_at) >= USAGE_API_MIN_MS;
        let token = due.then(|| config_dir().and_then(|d| crate::oauth::read_token(&d, now_ms()))).flatten();
        if let Some(token) = token {
            last_api_at = now_ms();
            match tokio::task::spawn_blocking(move || crate::oauth::fetch(&token)).await {
                Ok(Ok(u)) => {
                    lock(&shared.usage).apply_api(u.five_hour, u.seven_day, now_ms());
                    last_api_error.clear();
                }
                Ok(Err(e)) if e != last_api_error => {
                    log::warn!("usage endpoint: {e}");
                    last_api_error = e;
                }
                _ => {}
            }
        }
        let now = now_ms();
        if !lock(&shared.usage).has_fresh_exact(now) {
            if let Some(root) = projects_dir() {
                let s2 = shared.clone();
                let totals = tokio::task::spawn_blocking(move || lock(&s2.estimator).scan(&root, now_ms())).await;
                if let Ok(t) = totals {
                    lock(&shared.usage).set_estimate(t);
                }
            }
        }
        shared.mark_dirty();
        let busy = !lock(&shared.store).sessions.is_empty();
        tokio::time::sleep(if busy { USAGE_BUSY } else { USAGE_IDLE }).await;
    }
}

/// Low memory mode: destroys the island after the idle delay.
async fn idle_teardown(app: AppHandle, shared: Arc<Shared>) {
    loop {
        tokio::time::sleep(IDLE_CHECK).await;
        let (low_memory, idle_ms) = {
            let s = lock(&shared.settings);
            (s.low_memory, s.idle_minutes.max(1) * 60_000)
        };
        if shared.is_busy() {
            shared.touch_active();
            continue;
        }
        let idle_for = now_ms().saturating_sub(shared.last_active.load(Ordering::Relaxed));
        if low_memory && idle_for >= idle_ms && crate::window::get(&app).is_some() {
            log::info!("low memory mode: tearing down island after {}s idle", idle_for / 1000);
            crate::window::destroy(&app);
        }
    }
}

//! clawed backend: IPC server, session store, meters, tray and island window.

pub mod approvals;
pub mod commands;
pub mod format;
pub mod installer;
pub mod ipc;
pub mod liveness;
pub mod platform;
pub mod settings;
pub mod state;
pub mod store;
pub mod system;
pub mod tailer;
pub mod tasks;
pub mod transcript;
pub mod tray;
pub mod usage;
pub mod window;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::settings::Settings;
use crate::state::{lock, Shared};

struct AppHost(AppHandle);

impl ipc::Host for AppHost {
    fn wake_island(&self) {
        if window::get(&self.0).is_none() {
            window::ensure(&self.0);
        }
    }
}

/// Runs Tauri's async work on one background thread instead of a pool.
fn install_runtime() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name("clawed-rt")
        .build()
        .expect("tokio runtime");
    let handle = rt.handle().clone();
    std::thread::Builder::new()
        .name("clawed-rt".into())
        .spawn(move || rt.block_on(std::future::pending::<()>()))
        .expect("runtime thread");
    tauri::async_runtime::set(handle);
}

fn release_approvals(shared: &Shared) {
    let sessions = lock(&shared.approvals).release_all();
    let mut store = lock(&shared.store);
    for s in sessions {
        store.approval_finished(&s);
    }
}

pub fn run() {
    install_runtime();

    let settings_path = settings::settings_path();
    let shared = Arc::new(Shared::new(settings_path.clone(), Settings::load(&settings_path)));
    let autostarted = std::env::args().any(|a| a == "--autostarted");
    shared.autostarted.store(autostarted, Ordering::Relaxed);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            window::ensure(app);
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostarted"])))
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::decide,
            commands::set_island_size,
            commands::set_interactive,
            commands::open_settings,
            commands::get_settings,
            commands::save_settings,
            commands::installer_status,
            commands::installer_apply,
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            // Dev builds never register themselves as a login item.
            let first_run = !lock(&shared.settings).first_run_done;
            if first_run && !cfg!(debug_assertions) {
                if let Err(e) = handle.autolaunch().enable() {
                    log::warn!("enable autostart: {e}");
                }
                let mut s = lock(&shared.settings);
                s.first_run_done = true;
                let _ = s.save(&shared.settings_path);
            }

            tray::build(&handle)?;
            tasks::spawn_all(handle.clone(), shared.clone());

            match clawed_proto::pipe::name() {
                Ok(name) => {
                    let (s, host) = (shared.clone(), Arc::new(AppHost(handle.clone())));
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = ipc::serve(name, s, host).await {
                            log::error!("ipc server stopped: {e}");
                        }
                    });
                }
                Err(e) => log::error!("socket name: {e}"),
            }

            let hooks_installed = installer::read_settings(&installer::claude_settings_path())
                .map(|v| installer::is_installed(&v))
                .unwrap_or(false);
            if !autostarted {
                window::ensure(&handle);
                if !hooks_installed {
                    window::open_settings(&handle);
                }
            }
            shared.mark_dirty();
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building clawed");

    app.run(move |app, event| match event {
        // Closing the island or settings window must not quit the tray app.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            if let Some(shared) = app.try_state::<Arc<Shared>>() {
                release_approvals(&shared);
            }
        }
        _ => {}
    });
}

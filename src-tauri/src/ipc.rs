//! Local socket server: receives hook events and holds approval connections.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use clawed_proto::{now_ms, AppMsg, Event, HookMsg, PROTO_VERSION};
use interprocess::local_socket::tokio::{prelude::*, Stream};
use interprocess::local_socket::{ListenerOptions, Name};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

use crate::approvals::Pending;
use crate::state::{lock, Shared};

const MAX_LINE: u64 = 256 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Slightly above the hook's own 60 s wait.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(62);

/// What the IPC layer needs from the app shell.
pub trait Host: Send + Sync + 'static {
    /// Make sure the island exists (low memory mode may have destroyed it).
    fn wake_island(&self);
}

/// Accepts connections forever.
///
/// On Windows the pipe is created with `FILE_FLAG_FIRST_PIPE_INSTANCE` and
/// `PIPE_REJECT_REMOTE_CLIENTS` (interprocess defaults), plus a DACL that only
/// grants the current user. Creation fails if another process already owns
/// the name.
pub async fn serve(name: Name<'static>, shared: Arc<Shared>, host: Arc<dyn Host>) -> io::Result<()> {
    let opts = ListenerOptions::new().name(name).try_overwrite(true);
    #[cfg(windows)]
    let opts = {
        use interprocess::os::windows::local_socket::ListenerOptionsExt;
        use interprocess::os::windows::security_descriptor::SecurityDescriptor;
        let sddl = widestring::U16CString::from_str(clawed_proto::peer::pipe_sddl()?)
            .map_err(io::Error::other)?;
        opts.security_descriptor(SecurityDescriptor::deserialize(&sddl)?)
    };
    let listener = opts.create_tokio().map_err(|e| {
        io::Error::new(e.kind(), format!("cannot create socket (name already taken?): {e}"))
    })?;
    loop {
        let conn = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                log::warn!("ipc accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let (shared, host) = (shared.clone(), host.clone());
        tokio::spawn(async move {
            if let Err(e) = handle(conn, shared, host).await {
                log::debug!("ipc connection: {e}");
            }
        });
    }
}

async fn handle(conn: Stream, shared: Arc<Shared>, host: Arc<dyn Host>) -> io::Result<()> {
    if !conn.peer_creds().is_ok_and(|c| clawed_proto::peer::is_same_user(&c)) {
        log::warn!("ipc: rejected client running as another user");
        return Ok(());
    }
    let mut reader = BufReader::new(&conn);
    let mut line = String::new();
    {
        let mut limited = (&mut reader).take(MAX_LINE);
        let read = limited.read_line(&mut line);
        if tokio::time::timeout(READ_TIMEOUT, read).await.is_err() {
            return Ok(());
        }
    }
    let Ok(msg) = serde_json::from_str::<HookMsg>(&line) else { return Ok(()) };
    match msg {
        HookMsg::Event(ev) if ev.v == PROTO_VERSION => on_event(&shared, &*host, &ev),
        HookMsg::Status(st) if st.v == PROTO_VERSION => {
            let a = lock(&shared.store).apply_status(&st);
            let b = lock(&shared.usage).apply_status(&st);
            if a || b {
                shared.mark_dirty();
            }
        }
        HookMsg::Approval { id, event } if event.v == PROTO_VERSION => {
            on_approval(&conn, &mut reader, id, event, &shared, &*host).await?;
        }
        _ => {}
    }
    Ok(())
}

pub fn on_event(shared: &Shared, host: &dyn Host, ev: &Event) {
    let changed = lock(&shared.store).apply_event(ev);
    if matches!(ev.kind.as_str(), "PostToolUse" | "Stop" | "UserPromptSubmit" | "SessionStart" | "PreCompact") {
        lock(&shared.context_dirty).insert(ev.session_id.clone());
    }
    if ev.kind != "SessionEnd" && ev.kind != "Stop" {
        shared.touch_active();
        host.wake_island();
    }
    if changed {
        shared.mark_dirty();
    }
}

async fn on_approval(
    conn: &Stream,
    reader: &mut BufReader<&Stream>,
    id: String,
    event: Event,
    shared: &Shared,
    host: &dyn Host,
) -> io::Result<()> {
    on_event(shared, host, &event);
    let (tx, rx) = oneshot::channel();
    let repo = crate::store::repo_name(&event.cwd);
    let queued = lock(&shared.approvals).push(Pending {
        id: id.clone(),
        session_id: event.session_id.clone(),
        repo,
        tool: event.tool_name.clone().unwrap_or_default(),
        summary: event.tool_summary.clone(),
        created_at: now_ms(),
        reply: tx,
    });
    shared.mark_dirty();
    if queued {
        host.wake_island();
    }

    let mut probe = [0u8; 1];
    let reply = tokio::select! {
        r = rx => r.ok(),
        // The hook exits on its own timeout, or Claude Code answered in the
        // terminal and killed it: the connection closes.
        _ = reader.read(&mut probe) => None,
        _ = tokio::time::sleep(APPROVAL_TIMEOUT) => None,
    };
    if reply.is_none() {
        lock(&shared.approvals).forget(&id);
    }
    lock(&shared.store).approval_finished(&event.session_id);
    shared.mark_dirty();

    if let Some(msg) = reply {
        let mut out = serde_json::to_vec(&msg).map_err(io::Error::other)?;
        out.push(b'\n');
        let mut w = conn;
        w.write_all(&out).await?;
        w.flush().await?;
        if matches!(msg, AppMsg::Decision { .. }) {
            log::info!("approval {id} answered");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawed_proto::Behavior;
    use interprocess::local_socket::tokio::Stream as TokioStream;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct NoHost;
    impl Host for NoHost {
        fn wake_island(&self) {}
    }

    static N: AtomicUsize = AtomicUsize::new(0);

    fn test_name() -> Name<'static> {
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = format!("clawed-ipc-test-{}-{n}", std::process::id());
        #[cfg(windows)]
        {
            base.to_ns_name::<interprocess::local_socket::GenericNamespaced>().unwrap()
        }
        #[cfg(not(windows))]
        {
            std::env::temp_dir()
                .join(format!("{base}.sock"))
                .to_string_lossy()
                .into_owned()
                .to_fs_name::<interprocess::local_socket::GenericFilePath>()
                .unwrap()
        }
    }

    fn event(kind: &str) -> Event {
        let raw = serde_json::json!({
            "session_id": "s1", "cwd": "/w/proj", "hook_event_name": kind,
            "tool_name": "Bash", "tool_input": {"command": "rm -rf x"}
        });
        clawed_proto::strip_event(&raw, 7, now_ms()).unwrap()
    }

    async fn start() -> (Arc<Shared>, Name<'static>) {
        let shared = Arc::new(Shared::default());
        let name = test_name();
        let (s, n) = (shared.clone(), name.clone());
        tokio::spawn(async move { serve(n, s, Arc::new(NoHost)).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        (shared, name)
    }

    async fn send(name: &Name<'static>, msg: &HookMsg) -> TokioStream {
        let conn = TokioStream::connect(name.clone()).await.unwrap();
        let mut line = serde_json::to_vec(msg).unwrap();
        line.push(b'\n');
        (&conn).write_all(&line).await.unwrap();
        conn
    }

    async fn wait_for(cond: impl Fn() -> bool) {
        for _ in 0..100 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not met");
    }

    #[tokio::test]
    async fn event_reaches_store() {
        let (shared, name) = start().await;
        drop(send(&name, &HookMsg::Event(event("UserPromptSubmit"))).await);
        wait_for(|| lock(&shared.store).sessions.contains_key("s1")).await;
    }

    #[tokio::test]
    async fn unknown_version_dropped() {
        let (shared, name) = start().await;
        let mut ev = event("UserPromptSubmit");
        ev.v = 99;
        drop(send(&name, &HookMsg::Event(ev)).await);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(lock(&shared.store).sessions.is_empty());
    }

    #[tokio::test]
    async fn approval_decided_writes_decision() {
        let (shared, name) = start().await;
        let conn = send(&name, &HookMsg::Approval { id: "a1".into(), event: event("PermissionRequest") }).await;
        wait_for(|| !lock(&shared.approvals).is_empty()).await;
        assert_eq!(
            lock(&shared.store).sessions["s1"].state,
            crate::store::SessionState::AwaitingApproval
        );
        assert!(lock(&shared.approvals).decide("a1", Behavior::Allow).is_some());
        let mut line = String::new();
        BufReader::new(&conn).read_line(&mut line).await.unwrap();
        assert_eq!(
            serde_json::from_str::<AppMsg>(&line).unwrap(),
            AppMsg::Decision { id: "a1".into(), behavior: Behavior::Allow }
        );
        wait_for(|| lock(&shared.store).sessions["s1"].state == crate::store::SessionState::Working).await;
    }

    #[tokio::test]
    async fn release_all_unblocks() {
        let (shared, name) = start().await;
        let conn = send(&name, &HookMsg::Approval { id: "a2".into(), event: event("PermissionRequest") }).await;
        wait_for(|| !lock(&shared.approvals).is_empty()).await;
        lock(&shared.approvals).release_all();
        let mut line = String::new();
        BufReader::new(&conn).read_line(&mut line).await.unwrap();
        assert!(matches!(serde_json::from_str::<AppMsg>(&line).unwrap(), AppMsg::Release { .. }));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn second_server_cannot_squat_name() {
        let (_shared, name) = start().await;
        let err = serve(name, Arc::new(Shared::default()), Arc::new(NoHost)).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn hook_disconnect_forgets_approval() {
        let (shared, name) = start().await;
        let conn = send(&name, &HookMsg::Approval { id: "a3".into(), event: event("PermissionRequest") }).await;
        wait_for(|| !lock(&shared.approvals).is_empty()).await;
        drop(conn);
        wait_for(|| lock(&shared.approvals).is_empty()).await;
    }
}

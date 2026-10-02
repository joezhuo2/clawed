//! Queue of permission requests waiting on the island.

use std::collections::VecDeque;

use islet_proto::{AppMsg, Behavior};
use serde::Serialize;
use tokio::sync::oneshot;

pub struct Pending {
    pub id: String,
    pub session_id: String,
    pub repo: String,
    pub tool: String,
    pub summary: Option<String>,
    pub created_at: u64,
    pub reply: oneshot::Sender<AppMsg>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApprovalView {
    pub id: String,
    pub session_id: String,
    pub repo: String,
    pub tool: String,
    pub summary: Option<String>,
    pub created_at: u64,
}

#[derive(Default)]
pub struct Approvals {
    queue: VecDeque<Pending>,
    pub paused: bool,
}

impl Approvals {
    /// Queues a request. While paused the request is released at once and
    /// `false` is returned.
    pub fn push(&mut self, p: Pending) -> bool {
        if self.paused {
            let _ = p.reply.send(AppMsg::Release { id: p.id.clone() });
            return false;
        }
        self.queue.push_back(p);
        true
    }

    fn take(&mut self, id: &str) -> Option<Pending> {
        let i = self.queue.iter().position(|p| p.id == id)?;
        self.queue.remove(i)
    }

    /// Sends a decision. Returns the session id when the request existed.
    pub fn decide(&mut self, id: &str, behavior: Behavior) -> Option<String> {
        let p = self.take(id)?;
        let _ = p.reply.send(AppMsg::Decision { id: p.id.clone(), behavior });
        Some(p.session_id)
    }

    /// Drops a request whose hook went away (timeout or Claude answered).
    pub fn forget(&mut self, id: &str) -> Option<String> {
        self.take(id).map(|p| p.session_id)
    }

    /// Releases every pending request; returns their session ids.
    pub fn release_all(&mut self) -> Vec<String> {
        self.queue
            .drain(..)
            .map(|p| {
                let _ = p.reply.send(AppMsg::Release { id: p.id.clone() });
                p.session_id
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn views(&self) -> Vec<ApprovalView> {
        self.queue
            .iter()
            .map(|p| ApprovalView {
                id: p.id.clone(),
                session_id: p.session_id.clone(),
                repo: p.repo.clone(),
                tool: p.tool.clone(),
                summary: p.summary.clone(),
                created_at: p.created_at,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(id: &str) -> (Pending, oneshot::Receiver<AppMsg>) {
        let (tx, rx) = oneshot::channel();
        (
            Pending {
                id: id.into(),
                session_id: format!("s-{id}"),
                repo: "r".into(),
                tool: "Bash".into(),
                summary: Some("ls".into()),
                created_at: 0,
                reply: tx,
            },
            rx,
        )
    }

    #[test]
    fn decide_sends_decision_in_order() {
        let mut a = Approvals::default();
        let (p1, mut r1) = pending("1");
        let (p2, _r2) = pending("2");
        a.push(p1);
        a.push(p2);
        assert_eq!(a.views().iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["1", "2"]);
        assert_eq!(a.decide("1", Behavior::Allow).as_deref(), Some("s-1"));
        assert_eq!(r1.try_recv().unwrap(), AppMsg::Decision { id: "1".into(), behavior: Behavior::Allow });
        assert!(a.decide("1", Behavior::Deny).is_none());
        assert_eq!(a.views().len(), 1);
    }

    #[test]
    fn release_all_unblocks() {
        let mut a = Approvals::default();
        let (p1, mut r1) = pending("1");
        a.push(p1);
        assert_eq!(a.release_all(), vec!["s-1".to_string()]);
        assert_eq!(r1.try_recv().unwrap(), AppMsg::Release { id: "1".into() });
        assert!(a.is_empty());
    }

    #[test]
    fn paused_releases_immediately() {
        let mut a = Approvals { paused: true, ..Default::default() };
        let (p1, mut r1) = pending("1");
        assert!(!a.push(p1));
        assert_eq!(r1.try_recv().unwrap(), AppMsg::Release { id: "1".into() });
        assert!(a.is_empty());
    }

    #[test]
    fn forget_removes_without_reply() {
        let mut a = Approvals::default();
        let (p1, mut r1) = pending("1");
        a.push(p1);
        assert_eq!(a.forget("1").as_deref(), Some("s-1"));
        assert!(r1.try_recv().is_err());
    }
}

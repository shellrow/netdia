use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio_util::sync::CancellationToken;

pub const OP_PING: &str = "ping";
pub const OP_TRACEROUTE: &str = "traceroute";
pub const OP_PORTSCAN: &str = "portscan";
pub const OP_HOSTSCAN: &str = "hostscan";
pub const OP_NEIGHBORSCAN: &str = "neighborscan";
pub const OP_LATENCY: &str = "latency";
pub const OP_SPEEDTEST: &str = "speedtest";

struct Entry {
    instance: Arc<()>,
    run_id: String,
    token: CancellationToken,
    finished: CancellationToken,
    claimed: bool,
}

#[derive(Default)]
struct Registry {
    entries: Mutex<HashMap<String, Entry>>,
}

fn registry() -> &'static Arc<Registry> {
    static REGISTRY: OnceLock<Arc<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Arc::new(Registry::default()))
}

fn valid_kind(kind: &str) -> bool {
    matches!(
        kind,
        OP_PING
            | OP_TRACEROUTE
            | OP_PORTSCAN
            | OP_HOSTSCAN
            | OP_NEIGHBORSCAN
            | OP_LATENCY
            | OP_SPEEDTEST
    )
}

/// A claimed reservation. Dropping it ends only its own run, never a successor.
pub struct Operation {
    registry: Arc<Registry>,
    instance: Arc<()>,
    kind: String,
    run_id: String,
    pub token: CancellationToken,
    finished: CancellationToken,
}

impl Registry {
    fn prepare(&self, kind: &str, run_id: &str) -> Result<(), String> {
        if !valid_kind(kind) {
            return Err("Unknown diagnostic operation".into());
        }
        uuid::Uuid::parse_str(run_id).map_err(|_| "Invalid diagnostic run ID")?;
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        if entries
            .get(kind)
            .is_some_and(|entry| entry.run_id == run_id)
        {
            return Err("Diagnostic run ID is already reserved".into());
        }
        if let Some(old) = entries.remove(kind) {
            old.token.cancel();
        }
        entries.insert(
            kind.into(),
            Entry {
                instance: Arc::new(()),
                run_id: run_id.into(),
                token: CancellationToken::new(),
                finished: CancellationToken::new(),
                claimed: false,
            },
        );
        Ok(())
    }

    fn claim(self: &Arc<Self>, kind: &str, run_id: &str) -> Result<Operation, String> {
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let entry = entries
            .get_mut(kind)
            .filter(|entry| entry.run_id == run_id && !entry.claimed && !entry.token.is_cancelled())
            .ok_or("Diagnostic reservation was canceled, replaced, or already used")?;
        entry.claimed = true;
        Ok(Operation {
            registry: self.clone(),
            instance: entry.instance.clone(),
            kind: kind.into(),
            run_id: run_id.into(),
            token: entry.token.clone(),
            finished: entry.finished.clone(),
        })
    }

    async fn cancel(&self, kind: &str, run_id: &str) -> bool {
        let finished = {
            let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
            let Some(entry) = entries.get(kind).filter(|entry| entry.run_id == run_id) else {
                return false;
            };
            entry.token.cancel();
            if entry.claimed {
                // Keep claimed entries until Drop, so repeated cancel requests
                // all wait for the same worker instead of returning early.
                Some(entry.finished.clone())
            } else {
                entries.remove(kind);
                None
            }
        };
        if let Some(finished) = finished {
            finished.cancelled().await;
        }
        true
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        tracing::debug!(
            kind = self.kind,
            run_id = self.run_id,
            "diagnostic finished"
        );
        self.token.cancel();
        let mut entries = self
            .registry
            .entries
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if entries
            .get(&self.kind)
            .is_some_and(|entry| Arc::ptr_eq(&entry.instance, &self.instance))
        {
            entries.remove(&self.kind);
        }
        self.finished.cancel();
    }
}

pub fn claim_op(kind: &str, run_id: &str) -> Result<Operation, String> {
    registry().claim(kind, run_id)
}

pub async fn cancel_op(kind: &str, run_id: &str) -> bool {
    registry().cancel(kind, run_id).await
}

#[tauri::command]
pub fn prepare_operation(kind: String, run_id: String) -> Result<(), String> {
    registry().prepare(&kind, &run_id)
}

#[tauri::command]
pub async fn cancel_operation(kind: String, run_id: String) -> bool {
    cancel_op(&kind, &run_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    #[tokio::test]
    async fn cancel_before_claim_prevents_start() {
        let registry = Arc::new(Registry::default());
        let id = id();
        registry.prepare(OP_PING, &id).unwrap();
        assert!(registry.cancel(OP_PING, &id).await);
        assert!(registry.claim(OP_PING, &id).is_err());
    }

    #[tokio::test]
    async fn old_completion_and_cancel_cannot_touch_successor() {
        let registry = Arc::new(Registry::default());
        let old_id = id();
        registry.prepare(OP_PING, &old_id).unwrap();
        let old = registry.claim(OP_PING, &old_id).unwrap();
        let new_id = id();
        registry.prepare(OP_PING, &new_id).unwrap();
        assert!(old.token.is_cancelled());
        drop(old);
        assert!(!registry.cancel(OP_PING, &old_id).await);
        let new = registry.claim(OP_PING, &new_id).unwrap();
        assert!(!new.token.is_cancelled());
        drop(new);
        assert!(registry.entries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancellation_waits_for_claimed_worker_to_finish() {
        let registry = Arc::new(Registry::default());
        let id = id();
        registry.prepare(OP_SPEEDTEST, &id).unwrap();
        let operation = registry.claim(OP_SPEEDTEST, &id).unwrap();
        let token = operation.token.clone();
        let cancel = registry.cancel(OP_SPEEDTEST, &id);
        tokio::pin!(cancel);
        tokio::select! {
            biased;
            _ = &mut cancel => panic!("returned before worker cleanup"),
            _ = token.cancelled() => {}
        }
        drop(operation);
        assert!(cancel.await);
        assert!(!registry.cancel(OP_SPEEDTEST, &id).await);
    }

    #[tokio::test]
    async fn duplicate_cancellation_waits_and_reused_ids_cannot_remove_a_new_entry() {
        let registry = Arc::new(Registry::default());
        let first_id = id();
        registry.prepare(OP_PING, &first_id).unwrap();
        let first = registry.claim(OP_PING, &first_id).unwrap();
        let cancel_a = registry.cancel(OP_PING, &first_id);
        let cancel_b = registry.cancel(OP_PING, &first_id);
        tokio::pin!(cancel_a, cancel_b);
        assert!(futures::poll!(&mut cancel_a).is_pending());
        assert!(futures::poll!(&mut cancel_b).is_pending());
        registry.prepare(OP_PING, &id()).unwrap();
        registry.prepare(OP_PING, &first_id).unwrap();
        drop(first);
        assert!(cancel_a.await);
        assert!(cancel_b.await);
        let new = registry.claim(OP_PING, &first_id).unwrap();
        assert!(!new.token.is_cancelled());
    }

    #[test]
    fn internet_event_envelope_contains_identity_and_flat_metrics() {
        let payload = RunPayload {
            run_id: "reserved-id",
            payload: serde_json::json!({ "latency_ms": 12.0 }),
        };
        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            serde_json::json!({ "run_id": "reserved-id", "latency_ms": 12.0 })
        );
    }

    #[test]
    fn rejects_unknown_kinds_duplicate_reservations_and_duplicate_claims() {
        let registry = Arc::new(Registry::default());
        let id = id();
        assert!(registry.prepare("unknown", &id).is_err());
        assert!(registry.prepare(OP_PING, "invalid").is_err());
        registry.prepare(OP_PING, &id).unwrap();
        assert!(registry.prepare(OP_PING, &id).is_err());
        let _operation = registry.claim(OP_PING, &id).unwrap();
        assert!(registry.claim(OP_PING, &id).is_err());
    }
}

/// Attach the reserved identity to every event from an Internet diagnostic.
pub struct RunEmitter<'a> {
    pub app: &'a tauri::AppHandle,
    pub run_id: &'a str,
}

#[derive(serde::Serialize, Clone)]
struct RunPayload<'a, T> {
    run_id: &'a str,
    #[serde(flatten)]
    payload: T,
}

impl RunEmitter<'_> {
    pub fn emit<T: serde::Serialize + Clone>(&self, event: &str, payload: T) -> tauri::Result<()> {
        use tauri::Emitter;
        let result = self.app.emit(
            event,
            RunPayload {
                run_id: self.run_id,
                payload,
            },
        );
        if let Err(error) = &result {
            tracing::warn!(event, run_id = self.run_id, %error, "diagnostic event delivery failed");
        }
        result
    }
}

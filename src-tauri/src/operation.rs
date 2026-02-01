use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio_util::sync::CancellationToken;

type OperationMap = HashMap<String, CancellationToken>;

static OPS: OnceLock<Mutex<OperationMap>> = OnceLock::new();

pub const OP_PING: &str = "ping";
pub const OP_TRACEROUTE: &str = "traceroute";
pub const OP_PORTSCAN: &str = "portscan";
pub const OP_HOSTSCAN: &str = "hostscan";
pub const OP_NEIGHBORSCAN: &str = "neighborscan";

fn ops() -> &'static Mutex<OperationMap> {
    OPS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn start_op(key: &str) -> CancellationToken {
    let mut map = ops().lock().unwrap();

    if let Some(old) = map.remove(key) {
        old.cancel();
    }

    let token = CancellationToken::new();
    map.insert(key.to_string(), token.clone());
    token
}

pub fn cancel_op(key: &str) -> bool {
    let mut map = ops().lock().unwrap();
    if let Some(token) = map.remove(key) {
        token.cancel();
        true
    } else {
        false
    }
}

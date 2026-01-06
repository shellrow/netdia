use serde::{Deserialize, Serialize};
use std::fmt;

/// Status of probe
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ProbeStatusKind {
    /// Successfully completed
    Done,
    /// Interrupted by error
    Error,
    /// Execution time exceeds the configured timeout value
    Timeout,
}

impl ProbeStatusKind {
    /// Get the name of the status
    pub fn name(&self) -> String {
        match *self {
            ProbeStatusKind::Done => String::from("Done"),
            ProbeStatusKind::Error => String::from("Error"),
            ProbeStatusKind::Timeout => String::from("Timeout"),
        }
    }
}

impl fmt::Display for ProbeStatusKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeStatusKind::Done => write!(f, "Done"),
            ProbeStatusKind::Error => write!(f, "Error"),
            ProbeStatusKind::Timeout => write!(f, "Timeout"),
        }
    }
}

/// Status of probe
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProbeStatus {
    pub kind: ProbeStatusKind,
    pub message: String,
}

impl ProbeStatus {
    /// Create a new ProbeStatus with Done kind
    pub fn new() -> ProbeStatus {
        ProbeStatus {
            kind: ProbeStatusKind::Done,
            message: String::new(),
        }
    }
    /// Create a new ProbeStatus with Error kind and message
    pub fn with_error_message(message: String) -> ProbeStatus {
        ProbeStatus {
            kind: ProbeStatusKind::Error,
            message: message,
        }
    }
    /// Create a new ProbeStatus with Timeout kind and message
    pub fn with_timeout_message(message: String) -> ProbeStatus {
        ProbeStatus {
            kind: ProbeStatusKind::Timeout,
            message: message,
        }
    }
    pub fn is_ok(&self) -> bool {
        matches!(self.kind, ProbeStatusKind::Done)
    }
    pub fn is_error(&self) -> bool {
        matches!(self.kind, ProbeStatusKind::Error)
    }
    pub fn is_timeout(&self) -> bool {
        matches!(self.kind, ProbeStatusKind::Timeout)
    }
}

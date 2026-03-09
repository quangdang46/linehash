use thiserror::Error;

#[derive(Debug, Error)]
pub enum LinehashError {
    #[error("{command} is not implemented yet")]
    NotImplemented { command: &'static str },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl LinehashError {
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            LinehashError::NotImplemented { .. } => {
                Some("continue with the next planned implementation bead")
            }
            LinehashError::Io(_) => None,
            LinehashError::Json(_) => None,
        }
    }

    pub fn command(&self) -> Option<&'static str> {
        match self {
            LinehashError::NotImplemented { command } => Some(command),
            LinehashError::Io(_) | LinehashError::Json(_) => None,
        }
    }
}

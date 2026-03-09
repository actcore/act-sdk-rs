/// Result type for ACT tool functions.
pub type ActResult<T> = Result<T, ActError>;

/// Error type mapping to WIT `tool-error`.
#[derive(Debug, Clone)]
pub struct ActError {
    pub kind: String,
    pub message: String,
}

impl ActError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("std:not-found", message)
    }

    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new("std:invalid-args", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("std:internal", message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new("std:timeout", message)
    }
}

impl std::fmt::Display for ActError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ActError {}

/// Structured feedback from operations that can produce multiple messages.
///
/// This replaces direct `eprintln!` calls, allowing callers to decide how
/// to present feedback (CLI prints to stderr, TUI shows in status area,
/// library consumers can log or ignore).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feedback {
    /// Informational message (progress, status updates).
    Info(String),
    /// Warning - operation continued but something noteworthy occurred.
    Warning(String),
    /// Error - something failed (may or may not be fatal depending on context).
    Error(String),
}

impl Feedback {
    pub fn info(msg: impl Into<String>) -> Self {
        Self::Info(msg.into())
    }

    pub fn warning(msg: impl Into<String>) -> Self {
        Self::Warning(msg.into())
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error(msg.into())
    }

    /// Returns true if this is an error.
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Returns true if this is a warning.
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Warning(_))
    }

    /// Returns true if this is info.
    pub fn is_info(&self) -> bool {
        matches!(self, Self::Info(_))
    }

    /// Get the message text.
    pub fn message(&self) -> &str {
        match self {
            Self::Info(msg) | Self::Warning(msg) | Self::Error(msg) => msg,
        }
    }
}

impl std::fmt::Display for Feedback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info(msg) => write!(f, "{msg}"),
            Self::Warning(msg) => write!(f, "warning: {msg}"),
            Self::Error(msg) => write!(f, "error: {msg}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_constructors() {
        let info = Feedback::info("hello");
        assert!(info.is_info());
        assert_eq!(info.message(), "hello");

        let warn = Feedback::warning("careful");
        assert!(warn.is_warning());
        assert_eq!(warn.message(), "careful");

        let err = Feedback::error("oops");
        assert!(err.is_error());
        assert_eq!(err.message(), "oops");
    }

    #[test]
    fn feedback_display() {
        assert_eq!(Feedback::info("msg").to_string(), "msg");
        assert_eq!(Feedback::warning("msg").to_string(), "warning: msg");
        assert_eq!(Feedback::error("msg").to_string(), "error: msg");
    }
}

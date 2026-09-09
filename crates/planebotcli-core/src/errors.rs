//! Error type carrying the exit-code contract of the Python CLI
//! (Auth=2, NotFound=3, API=4, Validation=5).

use thiserror::Error;

pub const EXIT_AUTH: i32 = 2;
pub const EXIT_NOT_FOUND: i32 = 3;
pub const EXIT_API: i32 = 4;
pub const EXIT_VALIDATION: i32 = 5;

#[derive(Debug, Error)]
pub enum PlaneError {
    #[error("{message}")]
    Auth { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Api { message: String },
    #[error("{message}")]
    Validation {
        message: String,
        hint: Option<String>,
    },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl PlaneError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Auth { .. } => EXIT_AUTH,
            Self::NotFound { .. } => EXIT_NOT_FOUND,
            Self::Api { .. } => EXIT_API,
            Self::Validation { .. } => EXIT_VALIDATION,
            Self::Other(_) => 1,
        }
    }

    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Validation { hint, .. } => hint.as_deref(),
            _ => None,
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            hint: None,
        }
    }

    pub fn validation_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

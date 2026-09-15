use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    #[error("invalid tracking number: {0}")]
    InvalidInput(String),

    #[error("carrier request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("carrier temporarily rate limited requests")]
    RateLimited { retry_after: Option<Duration> },

    #[error("carrier denied automated access")]
    AccessDenied,

    #[error("carrier response format changed: {0}")]
    SourceChanged(String),

    #[error("carrier response belongs to another tracking identity")]
    IdentityMismatch,

    #[error("carrier service is temporarily unavailable (HTTP {0})")]
    SourceUnavailable(u16),
}

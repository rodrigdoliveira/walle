//! Carrier scraping primitives for Walle.
//!
//! The adapters deliberately keep transport, response classification, and
//! normalization together at the carrier boundary. Callers only receive a
//! verified [`LookupOutcome`] or a typed [`ScrapeError`].

pub mod dhl;
pub mod error;
pub mod hermes;
pub mod model;
pub mod source;

pub use dhl::DhlPaketSource;
pub use error::ScrapeError;
pub use hermes::HermesSource;
pub use model::{
    Carrier, DeliveryEstimate, InputField, LookupOutcome, NormalizedStatus, TrackingEvent,
    TrackingRequest, TrackingSnapshot,
};
pub use source::CarrierSource;

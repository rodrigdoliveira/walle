use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Carrier {
    DhlPaketDe,
    HermesDe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizedStatus {
    Unknown,
    Announced,
    InTransit,
    OutForDelivery,
    ReadyForPickup,
    DeliveryAttempted,
    Exception,
    Delivered,
    Returning,
    Returned,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputField {
    DestinationPostcode,
    ShipmentDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingRequest {
    pub tracking_number: String,
    pub destination_postcode: Option<String>,
    /// Carrier-formatted shipment date. DHL currently receives this as
    /// `postedDate`; the caller must preserve the format verified by its UI.
    pub shipment_date: Option<String>,
    pub international: bool,
}

impl TrackingRequest {
    pub fn new(tracking_number: impl Into<String>) -> Self {
        Self {
            tracking_number: tracking_number.into(),
            destination_postcode: None,
            shipment_date: None,
            international: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingEvent {
    pub source_id: Option<String>,
    pub raw_status: Option<String>,
    pub status: NormalizedStatus,
    pub description: String,
    pub location: Option<String>,
    /// Carrier-supplied timestamp preserved verbatim. Parsing and timezone
    /// certainty are handled by the application boundary once verified.
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryEstimate {
    pub from: Option<String>,
    pub until: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingSnapshot {
    pub carrier: Carrier,
    pub tracking_number: String,
    pub raw_status: Option<String>,
    pub status: NormalizedStatus,
    pub summary: Option<String>,
    pub sender: Option<String>,
    pub estimate: Option<DeliveryEstimate>,
    pub international_tracking_url: Option<String>,
    pub events: Vec<TrackingEvent>,
    pub parser_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum LookupOutcome {
    Found { snapshot: Box<TrackingSnapshot> },
    NotFound { reason: Option<String> },
    NeedsInput { fields: Vec<InputField> },
}

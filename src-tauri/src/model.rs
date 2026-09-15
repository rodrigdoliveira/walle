use serde::{Deserialize, Serialize};
use walle_tracking::{DeliveryEstimate, TrackingEvent};

#[derive(Debug, Clone, Serialize)]
pub struct ParcelRecord {
    pub id: String,
    pub name: String,
    pub tracking_number: String,
    pub carrier: String,
    pub destination_country: Option<String>,
    pub destination_postcode: Option<String>,
    pub shipment_date: Option<String>,
    pub international: bool,
    pub generation: i64,
    pub status: String,
    pub raw_status: Option<String>,
    pub summary: Option<String>,
    pub sender: Option<String>,
    pub estimate: Option<DeliveryEstimate>,
    pub international_tracking_url: Option<String>,
    pub events: Vec<TrackingEvent>,
    pub fetch_state: String,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_success_at: Option<String>,
    pub next_check_at: Option<String>,
    pub parser_version: Option<String>,
    pub notification_baseline: bool,
    pub last_notified_status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddParcelInput {
    pub name: String,
    pub tracking_number: String,
    pub carrier: String,
    pub destination_country: Option<String>,
    pub destination_postcode: Option<String>,
    pub shipment_date: Option<String>,
    pub international: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateParcelInput {
    pub id: String,
    pub name: String,
    pub destination_country: Option<String>,
    pub destination_postcode: Option<String>,
    pub shipment_date: Option<String>,
    pub international: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: String,
    pub notify_delivered: bool,
    pub notify_pickup: bool,
    pub notify_problems: bool,
    pub notify_out_for_delivery: bool,
    pub start_with_windows: bool,
    pub start_minimized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            notify_delivered: true,
            notify_pickup: true,
            notify_problems: true,
            notify_out_for_delivery: false,
            start_with_windows: false,
            start_minimized: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceHealth {
    pub carrier: String,
    pub label: String,
    pub enabled: bool,
    pub parser_version: String,
    pub last_success_at: Option<String>,
    pub limitation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefreshProgress {
    pub parcel_id: String,
    pub state: String,
}

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration as StdDuration,
};

use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use walle_tracking::{
    CarrierSource, DhlPaketSource, HermesSource, InputField, LookupOutcome, NormalizedStatus,
    ScrapeError, TrackingEvent, TrackingRequest,
};

use crate::{
    db::Database,
    model::{ParcelRecord, RefreshProgress},
};

const HOST_GAP: StdDuration = StdDuration::from_secs(30);
const MANUAL_COOLDOWN: Duration = Duration::minutes(5);

struct Inner {
    db: Database,
    dhl: DhlPaketSource,
    hermes: HermesSource,
    active: Mutex<HashSet<String>>,
    host_gates: AsyncMutex<HashMap<String, Arc<AsyncMutex<Option<std::time::Instant>>>>>,
    network_slots: Semaphore,
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

impl AppState {
    pub fn new(db: Database) -> Result<Self, String> {
        Ok(Self {
            inner: Arc::new(Inner {
                db,
                dhl: DhlPaketSource::new().map_err(|error| error.to_string())?,
                hermes: HermesSource::new().map_err(|error| error.to_string())?,
                active: Mutex::new(HashSet::new()),
                host_gates: AsyncMutex::new(HashMap::new()),
                network_slots: Semaphore::new(2),
            }),
        })
    }

    pub fn db(&self) -> &Database {
        &self.inner.db
    }

    pub async fn refresh(&self, app: &AppHandle, id: &str, manual: bool) -> Result<(), String> {
        {
            let mut active = self
                .inner
                .active
                .lock()
                .map_err(|_| "Refresh state is unavailable")?;
            if !active.insert(id.to_owned()) {
                return Ok(());
            }
        }
        let result = self.refresh_inner(app, id, manual).await;
        if let Ok(mut active) = self.inner.active.lock() {
            active.remove(id);
        }
        result
    }

    async fn refresh_inner(&self, app: &AppHandle, id: &str, manual: bool) -> Result<(), String> {
        let parcel = self.inner.db.get_parcel(id)?;
        if parcel.archived_at.is_some() {
            return Err("Restore this package before refreshing it".into());
        }
        if manual && recently_checked(parcel.last_checked_at.as_deref()) {
            return Err("This package was checked recently. Manual refresh is available after five minutes.".into());
        }

        let now = now_string();
        if !self.inner.db.mark_fetching(id, parcel.generation, &now)? {
            return Ok(());
        }
        emit_progress(app, id, "fetching");
        if let Ok(updated) = self.inner.db.get_parcel(id) {
            let _ = app.emit("parcel_updated", updated);
        }

        self.wait_for_host(&parcel.carrier).await;
        let _network_slot = self
            .inner
            .network_slots
            .acquire()
            .await
            .map_err(|_| "Refresh scheduler stopped")?;

        let request = TrackingRequest {
            tracking_number: parcel.tracking_number.clone(),
            destination_postcode: parcel.destination_postcode.clone(),
            shipment_date: parcel.shipment_date.clone(),
            international: parcel.international,
        };
        let result = match parcel.carrier.as_str() {
            "dhl_paket_de" => self.inner.dhl.lookup_request(&request).await,
            "hermes_de" => self.inner.hermes.lookup_request(&request).await,
            _ => return Err("This package uses an unsupported carrier".into()),
        };
        let completed_at = now_string();

        match result {
            Ok(LookupOutcome::Found { snapshot }) => {
                let events = merge_events(&parcel.events, &snapshot.events);
                let next_due = next_due(snapshot.status, &completed_at);
                let updated = self.inner.db.finish_success(
                    id,
                    parcel.generation,
                    &snapshot,
                    &events,
                    &completed_at,
                    next_due.as_deref(),
                )?;
                if let Some(updated) = updated {
                    maybe_notify(app, &self.inner.db, &parcel, &updated);
                    let _ = app.emit("parcel_updated", &updated);
                }
                emit_progress(app, id, "completed");
            }
            Ok(LookupOutcome::NotFound { reason: _ }) => {
                let next_due = add_time(&completed_at, Duration::hours(2));
                let updated = self.inner.db.finish_without_snapshot(
                    id,
                    parcel.generation,
                    "not_found",
                    Some("The carrier has not published tracking information for this number yet."),
                    &completed_at,
                    Some(&next_due),
                )?;
                emit_updated(app, updated);
                emit_progress(app, id, "completed");
            }
            Ok(LookupOutcome::NeedsInput { fields }) => {
                let message = fields
                    .iter()
                    .map(|field| match field {
                        InputField::DestinationPostcode => "destination postcode",
                        InputField::ShipmentDate => "shipment date",
                    })
                    .collect::<Vec<_>>()
                    .join(" and ");
                let updated = self.inner.db.finish_without_snapshot(
                    id,
                    parcel.generation,
                    "needs_input",
                    Some(&format!("The carrier requires {message}.")),
                    &completed_at,
                    None,
                )?;
                emit_updated(app, updated);
                emit_progress(app, id, "needs_input");
            }
            Err(error) => {
                let (fetch_state, message, next_due) = classify_error(&error, &completed_at);
                let updated = self.inner.db.finish_without_snapshot(
                    id,
                    parcel.generation,
                    fetch_state,
                    Some(&message),
                    &completed_at,
                    next_due.as_deref(),
                )?;
                emit_updated(app, updated);
                emit_progress(app, id, fetch_state);
                return Err(message);
            }
        }
        Ok(())
    }

    async fn wait_for_host(&self, carrier: &str) {
        let gate = {
            let mut gates = self.inner.host_gates.lock().await;
            gates
                .entry(carrier.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(None)))
                .clone()
        };
        let mut last_start = gate.lock().await;
        if let Some(last) = *last_start
            && let Some(wait) = HOST_GAP.checked_sub(last.elapsed())
        {
            tokio::time::sleep(wait).await;
        }
        *last_start = Some(std::time::Instant::now());
    }
}

pub fn start_background_scheduler(app: AppHandle, state: AppState) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(StdDuration::from_secs(8)).await;
        let mut ticker = tokio::time::interval(StdDuration::from_secs(60));
        loop {
            ticker.tick().await;
            let now = now_string();
            let Ok(ids) = state.db().due_parcel_ids(&now) else {
                continue;
            };
            for id in ids {
                let app = app.clone();
                let state = state.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = state.refresh(&app, &id, false).await;
                });
            }
        }
    });
}

pub fn queue_all(app: AppHandle, state: AppState) -> Result<usize, String> {
    let ids = state
        .db()
        .list_parcels()?
        .into_iter()
        .filter(|parcel| parcel.archived_at.is_none() && !is_terminal(&parcel.status))
        .map(|parcel| parcel.id)
        .collect::<Vec<_>>();
    for id in &ids {
        let app = app.clone();
        let state = state.clone();
        let id = id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = state.refresh(&app, &id, true).await;
        });
    }
    Ok(ids.len())
}

fn recently_checked(value: Option<&str>) -> bool {
    value
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
        .is_some_and(|value| OffsetDateTime::now_utc() - value < MANUAL_COOLDOWN)
}

fn merge_events(existing: &[TrackingEvent], incoming: &[TrackingEvent]) -> Vec<TrackingEvent> {
    let mut merged = existing.to_vec();
    let mut keys = existing.iter().map(event_key).collect::<HashSet<_>>();
    for event in incoming {
        if keys.insert(event_key(event)) {
            merged.push(event.clone());
        }
    }
    merged.sort_by_key(|event| std::cmp::Reverse(event_instant(event)));
    merged
}

fn event_key(event: &TrackingEvent) -> String {
    event.source_id.clone().unwrap_or_else(|| {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            event.raw_status.as_deref().unwrap_or_default(),
            event.description,
            event.location.as_deref().unwrap_or_default(),
            event.timestamp.as_deref().unwrap_or_default()
        )
    })
}

fn event_instant(event: &TrackingEvent) -> Option<OffsetDateTime> {
    event
        .timestamp
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
}

fn next_due(status: NormalizedStatus, now: &str) -> Option<String> {
    let interval = match status {
        NormalizedStatus::Unknown | NormalizedStatus::Announced => Duration::hours(2),
        NormalizedStatus::InTransit => Duration::hours(1),
        NormalizedStatus::OutForDelivery => Duration::minutes(30),
        NormalizedStatus::ReadyForPickup => Duration::hours(4),
        NormalizedStatus::DeliveryAttempted
        | NormalizedStatus::Exception
        | NormalizedStatus::Returning => Duration::hours(2),
        NormalizedStatus::Delivered | NormalizedStatus::Returned | NormalizedStatus::Cancelled => {
            return None;
        }
    };
    Some(add_time(now, interval))
}

fn classify_error(error: &ScrapeError, now: &str) -> (&'static str, String, Option<String>) {
    match error {
        ScrapeError::Http(http) if http.is_connect() => (
            "offline",
            "Could not reach the carrier. Cached tracking information is still available.".into(),
            Some(add_time(now, Duration::minutes(30))),
        ),
        ScrapeError::RateLimited { retry_after } => (
            "rate_limited",
            "The carrier requested a cooldown before the next update.".into(),
            Some(add_time(
                now,
                retry_after.map_or(Duration::hours(1), |duration| {
                    Duration::seconds(duration.as_secs() as i64)
                }),
            )),
        ),
        ScrapeError::AccessDenied => (
            "access_denied",
            "The carrier denied automated access. Open its website to check manually.".into(),
            None,
        ),
        ScrapeError::SourceChanged(_) | ScrapeError::IdentityMismatch => (
            "source_changed",
            "The carrier response changed and was rejected. Your cached status was preserved."
                .into(),
            None,
        ),
        ScrapeError::InvalidInput(message) => ("needs_input", message.clone(), None),
        ScrapeError::Http(_) | ScrapeError::SourceUnavailable(_) => (
            "source_error",
            "The carrier is temporarily unavailable. Walle will try again later.".into(),
            Some(add_time(now, Duration::minutes(30))),
        ),
    }
}

fn maybe_notify(app: &AppHandle, db: &Database, previous: &ParcelRecord, current: &ParcelRecord) {
    if !previous.notification_baseline
        || previous.status == current.status
        || previous.last_notified_status.as_deref() == Some(&current.status)
    {
        return;
    }
    let Ok(settings) = db.get_settings() else {
        return;
    };
    let allowed = match current.status.as_str() {
        "delivered" => settings.notify_delivered,
        "ready_for_pickup" => settings.notify_pickup,
        "out_for_delivery" => settings.notify_out_for_delivery,
        "delivery_attempted" | "exception" | "returning" | "returned" => settings.notify_problems,
        _ => false,
    };
    if !allowed {
        return;
    }
    let label = status_label(&current.status);
    if app
        .notification()
        .builder()
        .title(&current.name)
        .body(label)
        .show()
        .is_ok()
    {
        let _ = db.set_last_notified_status(&current.id, &current.status);
    }
}

fn status_label(status: &str) -> &'static str {
    match status {
        "delivered" => "Delivered",
        "ready_for_pickup" => "Ready for pickup",
        "out_for_delivery" => "Out for delivery",
        "delivery_attempted" => "Delivery attempted",
        "exception" => "The carrier reported a problem",
        "returning" => "Returning to sender",
        "returned" => "Return completed",
        _ => "Package updated",
    }
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "delivered" | "returned" | "cancelled")
}

fn emit_updated(app: &AppHandle, parcel: Option<ParcelRecord>) {
    if let Some(parcel) = parcel {
        let _ = app.emit("parcel_updated", parcel);
    }
}

fn emit_progress(app: &AppHandle, id: &str, state: &str) {
    let _ = app.emit(
        "refresh_progress",
        RefreshProgress {
            parcel_id: id.to_owned(),
            state: state.to_owned(),
        },
    );
}

pub fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC 3339 formatting is infallible")
}

fn add_time(now: &str, duration: Duration) -> String {
    OffsetDateTime::parse(now, &Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .saturating_add(duration)
        .format(&Rfc3339)
        .expect("RFC 3339 formatting is infallible")
}

#[cfg(test)]
mod tests {
    use super::{merge_events, next_due};
    use walle_tracking::{NormalizedStatus, TrackingEvent};

    #[test]
    fn event_merge_deduplicates_and_sorts_newest_first() {
        let earlier = TrackingEvent {
            source_id: None,
            raw_status: None,
            status: NormalizedStatus::Announced,
            description: "Announced".into(),
            location: None,
            timestamp: Some("2026-09-14T18:00:00Z".into()),
        };
        let later = TrackingEvent {
            source_id: None,
            raw_status: None,
            status: NormalizedStatus::InTransit,
            description: "Moving".into(),
            location: None,
            timestamp: Some("2026-09-14T19:00:00Z".into()),
        };
        let merged = merge_events(std::slice::from_ref(&earlier), &[earlier.clone(), later]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].description, "Moving");
    }

    #[test]
    fn terminal_statuses_have_no_next_due_time() {
        assert!(next_due(NormalizedStatus::Delivered, "2026-09-14T20:00:00Z").is_none());
        assert!(next_due(NormalizedStatus::InTransit, "2026-09-14T20:00:00Z").is_some());
    }
}

use std::{fs, path::PathBuf, time::Duration};

use rusqlite::{Connection, OptionalExtension, Row, params, types::Type};
use walle_tracking::{DeliveryEstimate, TrackingEvent, TrackingSnapshot};

use crate::model::{AddParcelInput, ParcelRecord, Settings, UpdateParcelInput};

const PARCEL_COLUMNS: &str = "
  id, name, tracking_number, carrier, destination_country, destination_postcode,
  shipment_date, international, generation, status, raw_status, summary, sender,
  estimate_json, international_tracking_url, events_json, fetch_state, last_error,
  created_at, updated_at, archived_at, last_checked_at, last_success_at, next_check_at,
  parser_version, notification_baseline, last_notified_status";

#[derive(Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn initialize(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create app data directory: {error}"))?;
        }
        let database = Self { path };
        let mut connection = database.connection()?;
        migrate(&mut connection)?;
        Ok(database)
    }

    fn connection(&self) -> Result<Connection, String> {
        let connection = Connection::open(&self.path).map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(db_error)?;
        Ok(connection)
    }

    pub fn list_parcels(&self) -> Result<Vec<ParcelRecord>, String> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {PARCEL_COLUMNS} FROM parcels ORDER BY created_at DESC, id"
            ))
            .map_err(db_error)?;
        let rows = statement.query_map([], row_to_parcel).map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    pub fn get_parcel(&self, id: &str) -> Result<ParcelRecord, String> {
        let connection = self.connection()?;
        connection
            .query_row(
                &format!("SELECT {PARCEL_COLUMNS} FROM parcels WHERE id = ?1"),
                [id],
                row_to_parcel,
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| "Package no longer exists".into())
    }

    pub fn add_parcel(&self, input: AddParcelInput, now: &str) -> Result<ParcelRecord, String> {
        let input = validate_add(input)?;
        let connection = self.connection()?;
        let duplicate: Option<String> = connection
            .query_row(
                "SELECT id FROM parcels WHERE carrier = ?1 AND tracking_number = ?2 COLLATE NOCASE LIMIT 1",
                params![input.carrier, input.tracking_number],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        if duplicate.is_some() {
            return Err("This carrier and tracking number already exist in Walle".into());
        }

        let id = uuid::Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO parcels (
                  id, name, tracking_number, carrier, destination_country, destination_postcode,
                  shipment_date, international, generation, status, events_json, fetch_state,
                  created_at, updated_at, next_check_at, notification_baseline
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 'unknown', '[]', 'queued', ?9, ?9, ?9, 0)",
                params![
                    id,
                    input.name,
                    input.tracking_number,
                    input.carrier,
                    input.destination_country,
                    input.destination_postcode,
                    input.shipment_date,
                    input.international,
                    now,
                ],
            )
            .map_err(|error| {
                if error.to_string().contains("UNIQUE constraint failed") {
                    "This carrier and tracking number already exist in Walle".into()
                } else {
                    db_error(error)
                }
            })?;
        self.get_parcel(&id)
    }

    pub fn update_parcel(
        &self,
        input: UpdateParcelInput,
        now: &str,
    ) -> Result<ParcelRecord, String> {
        let name = validate_name(&input.name)?;
        let country = validate_country(input.destination_country)?;
        let postcode = validate_postcode(input.destination_postcode)?;
        let shipment_date = validate_date(input.shipment_date)?;
        let current = self.get_parcel(&input.id)?;
        let request_changed = country != current.destination_country
            || postcode != current.destination_postcode
            || shipment_date != current.shipment_date
            || input.international != current.international;
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE parcels SET
                   name = ?2, destination_country = ?3, destination_postcode = ?4,
                   shipment_date = ?5, international = ?6, updated_at = ?7,
                   generation = generation + ?8,
                   fetch_state = CASE WHEN ?8 = 1 THEN 'queued' ELSE fetch_state END,
                   last_error = CASE WHEN ?8 = 1 THEN NULL ELSE last_error END,
                   next_check_at = CASE WHEN ?8 = 1 THEN ?7 ELSE next_check_at END
                 WHERE id = ?1",
                params![
                    input.id,
                    name,
                    country,
                    postcode,
                    shipment_date,
                    input.international,
                    now,
                    i64::from(request_changed),
                ],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err("Package no longer exists".into());
        }
        self.get_parcel(&input.id)
    }

    pub fn set_archived(
        &self,
        id: &str,
        archived: bool,
        now: &str,
    ) -> Result<ParcelRecord, String> {
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE parcels SET archived_at = ?2, updated_at = ?3,
                  next_check_at = CASE WHEN ?2 IS NULL AND status NOT IN ('delivered','returned','cancelled') THEN ?3 ELSE NULL END,
                  fetch_state = CASE WHEN ?2 IS NULL AND status NOT IN ('delivered','returned','cancelled') THEN 'queued' ELSE 'idle' END
                 WHERE id = ?1",
                params![id, archived.then_some(now), now],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err("Package no longer exists".into());
        }
        self.get_parcel(id)
    }

    pub fn delete_parcel(&self, id: &str) -> Result<(), String> {
        let connection = self.connection()?;
        let changed = connection
            .execute("DELETE FROM parcels WHERE id = ?1", [id])
            .map_err(db_error)?;
        if changed == 0 {
            return Err("Package no longer exists".into());
        }
        Ok(())
    }

    pub fn due_parcel_ids(&self, now: &str) -> Result<Vec<String>, String> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id FROM parcels
                 WHERE archived_at IS NULL
                   AND status NOT IN ('delivered','returned','cancelled')
                   AND next_check_at IS NOT NULL AND next_check_at <= ?1
                   AND fetch_state NOT IN ('needs_input','access_denied','source_changed')
                 ORDER BY next_check_at, id LIMIT 20",
            )
            .map_err(db_error)?;
        statement
            .query_map([now], |row| row.get(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)
    }

    pub fn mark_fetching(&self, id: &str, generation: i64, now: &str) -> Result<bool, String> {
        let connection = self.connection()?;
        connection
            .execute(
                "UPDATE parcels SET fetch_state = 'fetching', last_error = NULL,
                   last_checked_at = ?3, updated_at = ?3
                 WHERE id = ?1 AND generation = ?2 AND archived_at IS NULL",
                params![id, generation, now],
            )
            .map(|count| count == 1)
            .map_err(db_error)
    }

    pub fn finish_success(
        &self,
        id: &str,
        generation: i64,
        snapshot: &TrackingSnapshot,
        merged_events: &[TrackingEvent],
        now: &str,
        next_due: Option<&str>,
    ) -> Result<Option<ParcelRecord>, String> {
        let connection = self.connection()?;
        let estimate_json = snapshot
            .estimate
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(json_error)?;
        let events_json = serde_json::to_string(merged_events).map_err(json_error)?;
        let status = status_key(snapshot.status);
        let changed = connection
            .execute(
                "UPDATE parcels SET status = ?3, raw_status = ?4, summary = ?5, sender = ?6,
                   estimate_json = ?7, international_tracking_url = ?8, events_json = ?9,
                   fetch_state = 'idle', last_error = NULL, updated_at = ?10,
                   last_checked_at = ?10, last_success_at = ?10, next_check_at = ?11,
                   parser_version = ?12, notification_baseline = 1
                 WHERE id = ?1 AND generation = ?2 AND archived_at IS NULL",
                params![
                    id,
                    generation,
                    status,
                    snapshot.raw_status,
                    snapshot.summary,
                    snapshot.sender,
                    estimate_json,
                    snapshot.international_tracking_url,
                    events_json,
                    now,
                    next_due,
                    snapshot.parser_version,
                ],
            )
            .map_err(db_error)?;
        (changed == 1).then(|| self.get_parcel(id)).transpose()
    }

    pub fn finish_without_snapshot(
        &self,
        id: &str,
        generation: i64,
        fetch_state: &str,
        message: Option<&str>,
        now: &str,
        next_due: Option<&str>,
    ) -> Result<Option<ParcelRecord>, String> {
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE parcels SET fetch_state = ?3, last_error = ?4, updated_at = ?5,
                   last_checked_at = ?5, next_check_at = ?6
                 WHERE id = ?1 AND generation = ?2 AND archived_at IS NULL",
                params![id, generation, fetch_state, message, now, next_due],
            )
            .map_err(db_error)?;
        (changed == 1).then(|| self.get_parcel(id)).transpose()
    }

    pub fn set_last_notified_status(&self, id: &str, status: &str) -> Result<(), String> {
        let connection = self.connection()?;
        connection
            .execute(
                "UPDATE parcels SET last_notified_status = ?2 WHERE id = ?1",
                params![id, status],
            )
            .map(|_| ())
            .map_err(db_error)
    }

    pub fn get_settings(&self) -> Result<Settings, String> {
        let connection = self.connection()?;
        let json: Option<String> = connection
            .query_row("SELECT value FROM settings WHERE key = 'app'", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(db_error)?;
        json.map(|value| serde_json::from_str(&value).map_err(json_error))
            .transpose()
            .map(|settings| settings.unwrap_or_default())
    }

    pub fn set_settings(&self, settings: &Settings) -> Result<(), String> {
        validate_settings(settings)?;
        let connection = self.connection()?;
        let json = serde_json::to_string(settings).map_err(json_error)?;
        connection
            .execute(
                "INSERT INTO settings(key, value) VALUES ('app', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [json],
            )
            .map(|_| ())
            .map_err(db_error)
    }

    pub fn last_source_success(&self, carrier: &str) -> Result<Option<String>, String> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT MAX(last_success_at) FROM parcels WHERE carrier = ?1",
                [carrier],
                |row| row.get(0),
            )
            .map_err(db_error)
    }
}

fn migrate(connection: &mut Connection) -> Result<(), String> {
    let transaction = connection.transaction().map_err(db_error)?;
    transaction
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS parcels (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              tracking_number TEXT NOT NULL,
              carrier TEXT NOT NULL,
              destination_country TEXT,
              destination_postcode TEXT,
              shipment_date TEXT,
              international INTEGER NOT NULL DEFAULT 0,
              generation INTEGER NOT NULL DEFAULT 1,
              status TEXT NOT NULL DEFAULT 'unknown',
              raw_status TEXT,
              summary TEXT,
              sender TEXT,
              estimate_json TEXT,
              international_tracking_url TEXT,
              events_json TEXT NOT NULL DEFAULT '[]',
              fetch_state TEXT NOT NULL DEFAULT 'idle',
              last_error TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              archived_at TEXT,
              last_checked_at TEXT,
              last_success_at TEXT,
              next_check_at TEXT,
              parser_version TEXT,
              notification_baseline INTEGER NOT NULL DEFAULT 0,
              last_notified_status TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS parcels_identity ON parcels(carrier, tracking_number COLLATE NOCASE);
            CREATE INDEX IF NOT EXISTS parcels_due ON parcels(archived_at, next_check_at);
            CREATE INDEX IF NOT EXISTS parcels_status ON parcels(status, archived_at);
            CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS source_state (
              carrier TEXT PRIMARY KEY,
              enabled INTEGER NOT NULL DEFAULT 1,
              paused_reason TEXT,
              next_allowed_at TEXT,
              updated_at TEXT NOT NULL
            );
            PRAGMA user_version = 1;",
        )
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)
}

fn row_to_parcel(row: &Row<'_>) -> rusqlite::Result<ParcelRecord> {
    let estimate_json: Option<String> = row.get(13)?;
    let estimate = estimate_json
        .map(|value| serde_json::from_str::<DeliveryEstimate>(&value))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(13, Type::Text, Box::new(error))
        })?;
    let events_json: String = row.get(15)?;
    let events = serde_json::from_str::<Vec<TrackingEvent>>(&events_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(15, Type::Text, Box::new(error))
    })?;
    Ok(ParcelRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        tracking_number: row.get(2)?,
        carrier: row.get(3)?,
        destination_country: row.get(4)?,
        destination_postcode: row.get(5)?,
        shipment_date: row.get(6)?,
        international: row.get(7)?,
        generation: row.get(8)?,
        status: row.get(9)?,
        raw_status: row.get(10)?,
        summary: row.get(11)?,
        sender: row.get(12)?,
        estimate,
        international_tracking_url: row.get(14)?,
        events,
        fetch_state: row.get(16)?,
        last_error: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
        archived_at: row.get(20)?,
        last_checked_at: row.get(21)?,
        last_success_at: row.get(22)?,
        next_check_at: row.get(23)?,
        parser_version: row.get(24)?,
        notification_baseline: row.get(25)?,
        last_notified_status: row.get(26)?,
    })
}

fn validate_add(mut input: AddParcelInput) -> Result<AddParcelInput, String> {
    input.name = validate_name(&input.name)?;
    input.tracking_number = input.tracking_number.trim().to_owned();
    if input.tracking_number.is_empty() || input.tracking_number.len() > 100 {
        return Err("Tracking number must contain 1 to 100 characters".into());
    }
    if input.tracking_number.chars().any(char::is_control) {
        return Err("Tracking number contains unsupported control characters".into());
    }
    if !matches!(input.carrier.as_str(), "dhl_paket_de" | "hermes_de") {
        return Err("Choose DHL or Hermes".into());
    }
    input.destination_country = validate_country(input.destination_country)?;
    input.destination_postcode = validate_postcode(input.destination_postcode)?;
    input.shipment_date = validate_date(input.shipment_date)?;
    Ok(input)
}

fn validate_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 || value.chars().any(char::is_control) {
        return Err("Package name must contain 1 to 120 printable characters".into());
    }
    Ok(value.to_owned())
}

fn validate_country(value: Option<String>) -> Result<Option<String>, String> {
    let value = value
        .map(|item| item.trim().to_ascii_uppercase())
        .filter(|item| !item.is_empty());
    if value.as_ref().is_some_and(|item| {
        item.len() != 2
            || !item
                .chars()
                .all(|character| character.is_ascii_alphabetic())
    }) {
        return Err("Destination country must be a two-letter country code".into());
    }
    Ok(value)
}

fn validate_postcode(value: Option<String>) -> Result<Option<String>, String> {
    let value = value
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty());
    if value
        .as_ref()
        .is_some_and(|item| item.len() > 16 || item.chars().any(char::is_control))
    {
        return Err("Destination postcode is too long or contains unsupported characters".into());
    }
    Ok(value)
}

fn validate_date(value: Option<String>) -> Result<Option<String>, String> {
    let value = value
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty());
    if value.as_ref().is_some_and(|item| {
        item.len() != 10
            || item
                .chars()
                .enumerate()
                .any(|(index, character)| match index {
                    4 | 7 => character != '-',
                    _ => !character.is_ascii_digit(),
                })
    }) {
        return Err("Shipment date must use YYYY-MM-DD".into());
    }
    Ok(value)
}

fn validate_settings(settings: &Settings) -> Result<(), String> {
    if !matches!(settings.theme.as_str(), "system" | "light" | "dark") {
        return Err("Theme must be system, light, or dark".into());
    }
    Ok(())
}

pub fn status_key(status: walle_tracking::NormalizedStatus) -> &'static str {
    use walle_tracking::NormalizedStatus as Status;
    match status {
        Status::Unknown => "unknown",
        Status::Announced => "announced",
        Status::InTransit => "in_transit",
        Status::OutForDelivery => "out_for_delivery",
        Status::ReadyForPickup => "ready_for_pickup",
        Status::DeliveryAttempted => "delivery_attempted",
        Status::Exception => "exception",
        Status::Delivered => "delivered",
        Status::Returning => "returning",
        Status::Returned => "returned",
        Status::Cancelled => "cancelled",
    }
}

fn db_error(error: rusqlite::Error) -> String {
    format!("Local database error: {error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("Stored tracking data is invalid: {error}")
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::model::AddParcelInput;

    fn test_database() -> (Database, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("walle-test-{}.sqlite3", uuid::Uuid::new_v4()));
        (Database::initialize(path.clone()).unwrap(), path)
    }

    #[test]
    fn parcel_crud_preserves_leading_zeroes_and_archive_state() {
        let (database, path) = test_database();
        let parcel = database
            .add_parcel(
                AddParcelInput {
                    name: "  Shelves  ".into(),
                    tracking_number: "88888888888888".into(),
                    carrier: "hermes_de".into(),
                    destination_country: Some("es".into()),
                    destination_postcode: None,
                    shipment_date: None,
                    international: true,
                },
                "2026-09-14T20:00:00Z",
            )
            .unwrap();
        assert_eq!(parcel.name, "Shelves");
        assert_eq!(parcel.tracking_number, "88888888888888");
        assert_eq!(parcel.destination_country.as_deref(), Some("ES"));
        assert_eq!(database.list_parcels().unwrap().len(), 1);

        let archived = database
            .set_archived(&parcel.id, true, "2026-09-14T21:00:00Z")
            .unwrap();
        assert!(archived.archived_at.is_some());
        assert!(archived.next_check_at.is_none());
        database.delete_parcel(&parcel.id).unwrap();
        assert!(database.list_parcels().unwrap().is_empty());
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn duplicate_identity_is_rejected() {
        let (database, path) = test_database();
        let input = AddParcelInput {
            name: "Parcel".into(),
            tracking_number: "ABC12345678".into(),
            carrier: "dhl_paket_de".into(),
            destination_country: None,
            destination_postcode: None,
            shipment_date: None,
            international: false,
        };
        database
            .add_parcel(input.clone(), "2026-09-14T20:00:00Z")
            .unwrap();
        assert!(database.add_parcel(input, "2026-09-14T20:01:00Z").is_err());
        drop(database);
        let _ = std::fs::remove_file(path);
    }
}

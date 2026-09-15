use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    model::{AddParcelInput, ParcelRecord, Settings, SourceHealth, UpdateParcelInput},
    scheduler::{AppState, now_string, queue_all},
};

#[tauri::command]
pub fn list_parcels(state: State<'_, AppState>) -> Result<Vec<ParcelRecord>, String> {
    state.db().list_parcels()
}

#[tauri::command]
pub fn get_parcel(id: String, state: State<'_, AppState>) -> Result<ParcelRecord, String> {
    state.db().get_parcel(&id)
}

#[tauri::command]
pub fn add_parcel(
    input: AddParcelInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ParcelRecord, String> {
    let parcel = state.db().add_parcel(input, &now_string())?;
    let _ = app.emit("parcel_updated", &parcel);
    let id = parcel.id.clone();
    let app_for_refresh = app.clone();
    let state_for_refresh = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = state_for_refresh
            .refresh(&app_for_refresh, &id, false)
            .await;
    });
    Ok(parcel)
}

#[tauri::command]
pub fn update_parcel(
    input: UpdateParcelInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ParcelRecord, String> {
    let previous = state.db().get_parcel(&input.id)?;
    let parcel = state.db().update_parcel(input, &now_string())?;
    let _ = app.emit("parcel_updated", &parcel);
    if previous.generation != parcel.generation {
        let id = parcel.id.clone();
        let app_for_refresh = app.clone();
        let state_for_refresh = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            let _ = state_for_refresh
                .refresh(&app_for_refresh, &id, false)
                .await;
        });
    }
    Ok(parcel)
}

#[tauri::command]
pub fn archive_parcel(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ParcelRecord, String> {
    let parcel = state.db().set_archived(&id, true, &now_string())?;
    let _ = app.emit("parcel_updated", &parcel);
    Ok(parcel)
}

#[tauri::command]
pub fn restore_parcel(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ParcelRecord, String> {
    let parcel = state.db().set_archived(&id, false, &now_string())?;
    let _ = app.emit("parcel_updated", &parcel);
    Ok(parcel)
}

#[tauri::command]
pub fn delete_parcel(id: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.db().delete_parcel(&id)?;
    let _ = app.emit("parcel_deleted", id);
    Ok(())
}

#[tauri::command]
pub async fn refresh_parcel(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.refresh(&app, &id, true).await
}

#[tauri::command]
pub fn refresh_all(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    queue_all(app, state.inner().clone())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state.db().get_settings()
}

#[tauri::command]
pub fn update_settings(
    settings: Settings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    if settings.start_with_windows {
        app.autolaunch()
            .enable()
            .map_err(|error| format!("Could not enable Windows startup: {error}"))?;
    } else {
        app.autolaunch()
            .disable()
            .map_err(|error| format!("Could not disable Windows startup: {error}"))?;
    }
    state.db().set_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn get_source_health(state: State<'_, AppState>) -> Result<Vec<SourceHealth>, String> {
    Ok(vec![
        SourceHealth {
            carrier: "dhl_paket_de".into(),
            label: "DHL".into(),
            enabled: true,
            parser_version: walle_tracking::dhl::DHL_PARSER_VERSION.into(),
            last_success_at: state.db().last_source_success("dhl_paket_de")?,
            limitation: "Validated with one delivered international parcel; optional-input success remains unverified.".into(),
        },
        SourceHealth {
            carrier: "hermes_de".into(),
            label: "Hermes Germany".into(),
            enabled: true,
            parser_version: walle_tracking::hermes::HERMES_PARSER_VERSION.into(),
            last_success_at: state.db().last_source_success("hermes_de")?,
            limitation: "Validated through announced state and partner handoff; later states remain fixture-tested.".into(),
        },
    ])
}

#[tauri::command]
pub fn open_tracking_page(
    id: String,
    partner: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let parcel = state.db().get_parcel(&id)?;
    let url = if partner.unwrap_or(false) {
        let url = parcel
            .international_tracking_url
            .ok_or_else(|| "This package has no partner tracking page".to_owned())?;
        const ALLOWED_PARTNER_PREFIXES: [&str; 3] = [
            "https://www.correos.es/",
            "https://www.dhl.com/",
            "https://nolp.dhl.de/",
        ];
        if !ALLOWED_PARTNER_PREFIXES
            .iter()
            .any(|prefix| url.starts_with(prefix))
        {
            return Err("The carrier supplied an unsupported partner tracking host".into());
        }
        url
    } else {
        let encoded = urlencoding::encode(&parcel.tracking_number);
        match parcel.carrier.as_str() {
            "dhl_paket_de" => format!(
                "https://www.dhl.de/de/privatkunden/dhl-sendungsverfolgung.html?piececode={encoded}"
            ),
            "hermes_de" => format!(
                "https://www.myhermes.de/empfangen/sendungsverfolgung/sendungsinformation#{encoded}"
            ),
            _ => return Err("This package uses an unsupported carrier".into()),
        }
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| format!("Could not open carrier website: {error}"))
}

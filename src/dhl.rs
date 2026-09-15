use async_trait::async_trait;
use reqwest::{
    StatusCode,
    header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, REFERER},
    redirect::Policy,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::ScrapeError,
    model::{
        Carrier, DeliveryEstimate, InputField, LookupOutcome, NormalizedStatus, TrackingEvent,
        TrackingRequest, TrackingSnapshot,
    },
    source::{CarrierSource, first_string, plain_text, retry_after, validate_tracking_number},
};

const DEFAULT_DATA_BASE: &str = "https://www.dhl.de/int-verfolgen/data";
const DEFAULT_REFERER: &str = "https://www.dhl.de/de/privatkunden/dhl-sendungsverfolgung.html";
pub const DHL_PARSER_VERSION: &str = "dhl-paket-de/2026-09-14.3";

#[derive(Clone)]
pub struct DhlPaketSource {
    client: reqwest::Client,
    data_base: String,
    referer: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DhlConfig {
    verfolgen_csrf_token: String,
}

impl DhlPaketSource {
    pub fn new() -> Result<Self, ScrapeError> {
        Self::with_endpoints(DEFAULT_DATA_BASE, DEFAULT_REFERER)
    }

    /// Creates a source with explicit endpoints. This is public so integration
    /// tests can exercise the complete request flow against a local server.
    pub fn with_endpoints(data_base: &str, referer: &str) -> Result<Self, ScrapeError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Walle/", env!("CARGO_PKG_VERSION")))
            .cookie_store(true)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .redirect(Policy::limited(3))
            .build()?;

        Ok(Self {
            client,
            data_base: data_base.trim_end_matches('/').to_owned(),
            referer: referer.to_owned(),
        })
    }

    async fn config(&self) -> Result<DhlConfig, ScrapeError> {
        let response = self
            .client
            .get(format!("{}/config", self.data_base))
            .query(&[("domain", "de"), ("language", "en")])
            .header(ACCEPT, "application/json")
            .header(REFERER, &self.referer)
            .send()
            .await
            .map_err(|error| ScrapeError::Http(error.without_url()))?;

        classify_http_status(response.status(), response.headers())?;
        let body = bounded_body(response, 1_048_576).await?;
        let config: DhlConfig = serde_json::from_slice(&body).map_err(|error| {
            ScrapeError::SourceChanged(format!("invalid DHL configuration JSON: {error}"))
        })?;
        if config.verfolgen_csrf_token.trim().is_empty() {
            return Err(ScrapeError::SourceChanged(
                "DHL configuration omitted its CSRF token".into(),
            ));
        }
        Ok(config)
    }

    async fn search(
        &self,
        tracking_number: &str,
        config: &DhlConfig,
    ) -> Result<Vec<u8>, ScrapeError> {
        let csrf_header = HeaderName::from_static("verfolgen-csrf-token");
        let workgroup_header = HeaderName::from_static("verfolgen-wg");
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            csrf_header,
            HeaderValue::from_str(&config.verfolgen_csrf_token).map_err(|_| {
                ScrapeError::SourceChanged("DHL returned an invalid CSRF token".into())
            })?,
        );
        headers.insert(workgroup_header, HeaderValue::from_static("0"));

        let response = self
            .client
            .get(format!("{}/search", self.data_base))
            .query(&[
                ("piececode", tracking_number),
                ("noRedirect", "true"),
                ("language", "en"),
            ])
            .headers(headers)
            .header(REFERER, &self.referer)
            .send()
            .await
            .map_err(|error| ScrapeError::Http(error.without_url()))?;

        classify_http_status(response.status(), response.headers())?;
        bounded_body(response, 5 * 1_048_576).await
    }

    async fn shipment_details(
        &self,
        config: &DhlConfig,
        payload: &Value,
    ) -> Result<Vec<u8>, ScrapeError> {
        let csrf_header = HeaderName::from_static("verfolgen-csrf-token");
        let response = self
            .client
            .post(format!("{}/shipment", self.data_base))
            .query(&[("language", "en")])
            .header(ACCEPT, HeaderValue::from_static("application/json"))
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .header(
                csrf_header,
                HeaderValue::from_str(&config.verfolgen_csrf_token).map_err(|_| {
                    ScrapeError::SourceChanged("DHL returned an invalid CSRF token".into())
                })?,
            )
            .header(REFERER, &self.referer)
            .json(payload)
            .send()
            .await
            .map_err(|error| ScrapeError::Http(error.without_url()))?;

        classify_http_status(response.status(), response.headers())?;
        bounded_body(response, 5 * 1_048_576).await
    }
}

#[async_trait]
impl CarrierSource for DhlPaketSource {
    async fn lookup_request(
        &self,
        request: &TrackingRequest,
    ) -> Result<LookupOutcome, ScrapeError> {
        let tracking_number = validate_tracking_number(&request.tracking_number)?;
        let config = self.config().await?;
        let body = self.search(&tracking_number, &config).await?;
        let mut outcome = parse_search_response(&body, &tracking_number)?;

        for _ in 0..2 {
            let LookupOutcome::NeedsInput { fields } = &outcome else {
                return Ok(outcome);
            };

            let payload = if fields.contains(&InputField::DestinationPostcode) {
                let Some(postcode) = request.destination_postcode.as_deref() else {
                    return Ok(outcome);
                };
                let postcode = validate_postcode(postcode)?;
                serde_json::json!({
                    "piececode": tracking_number,
                    "zip": postcode,
                    "international": request.international,
                })
            } else if fields.contains(&InputField::ShipmentDate) {
                let Some(shipment_date) = request.shipment_date.as_deref() else {
                    return Ok(outcome);
                };
                let shipment_date = validate_shipment_date(shipment_date)?;
                serde_json::json!({
                    "piececode": tracking_number,
                    "postedDate": shipment_date,
                    "international": request.international,
                })
            } else {
                return Ok(outcome);
            };

            let body = self.shipment_details(&config, &payload).await?;
            outcome = parse_search_response(&body, &tracking_number)?;
        }

        Ok(outcome)
    }
}

fn validate_postcode(value: &str) -> Result<&str, ScrapeError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 16
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == ' ' || character == '-'
        })
    {
        return Err(ScrapeError::InvalidInput(
            "destination postcode has an unsupported format".into(),
        ));
    }
    Ok(value)
}

fn validate_shipment_date(value: &str) -> Result<&str, ScrapeError> {
    let value = value.trim();
    let valid = value.len() == 10
        && value
            .chars()
            .enumerate()
            .all(|(index, character)| match index {
                4 | 7 => character == '-',
                _ => character.is_ascii_digit(),
            });
    if !valid {
        return Err(ScrapeError::InvalidInput(
            "shipment date must use YYYY-MM-DD".into(),
        ));
    }
    Ok(value)
}

fn classify_http_status(status: StatusCode, headers: &HeaderMap) -> Result<(), ScrapeError> {
    match status {
        StatusCode::TOO_MANY_REQUESTS => Err(ScrapeError::RateLimited {
            retry_after: retry_after(headers),
        }),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ScrapeError::AccessDenied),
        status if status.is_server_error() => Err(ScrapeError::SourceUnavailable(status.as_u16())),
        status if !status.is_success() => Err(ScrapeError::SourceChanged(format!(
            "unexpected DHL HTTP status {}",
            status.as_u16()
        ))),
        _ => Ok(()),
    }
}

async fn bounded_body(
    response: reqwest::Response,
    maximum_size: usize,
) -> Result<Vec<u8>, ScrapeError> {
    if response
        .content_length()
        .is_some_and(|size| size > maximum_size as u64)
    {
        return Err(ScrapeError::SourceChanged(
            "DHL response exceeded the size limit".into(),
        ));
    }
    let body = response.bytes().await?;
    if body.len() > maximum_size {
        return Err(ScrapeError::SourceChanged(
            "DHL response exceeded the size limit".into(),
        ));
    }
    Ok(body.to_vec())
}

pub fn parse_search_response(
    body: &[u8],
    requested_number: &str,
) -> Result<LookupOutcome, ScrapeError> {
    if looks_like_html(body) {
        let text = String::from_utf8_lossy(body).to_lowercase();
        if text.contains("tracking attempt has been blocked")
            || text.contains("zugriffsversuch wurde blockiert")
        {
            return Err(ScrapeError::AccessDenied);
        }
        return Err(ScrapeError::SourceChanged(
            "DHL returned HTML instead of tracking JSON".into(),
        ));
    }

    let root: Value = serde_json::from_slice(body).map_err(|error| {
        ScrapeError::SourceChanged(format!("invalid DHL tracking JSON: {error}"))
    })?;

    if root
        .get("isRateLimited")
        .or_else(|| root.get("rateLimited"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Err(ScrapeError::RateLimited { retry_after: None });
    }

    let shipments = root
        .get("sendungen")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ScrapeError::SourceChanged("DHL response omitted the sendungen array".into())
        })?;

    if shipments.is_empty() {
        return Err(ScrapeError::SourceChanged(
            "DHL returned an empty unclassified shipment array".into(),
        ));
    }

    let shipment = shipments
        .iter()
        .find(|shipment| shipment_matches(shipment, requested_number))
        .ok_or(ScrapeError::IdentityMismatch)?;

    if shipment
        .get("blockTime")
        .is_some_and(|value| !value.is_null())
    {
        return Err(ScrapeError::RateLimited { retry_after: None });
    }
    if shipment
        .get("shipmentSearchBackendError")
        .is_some_and(|value| !value.is_null() && value != false)
    {
        return Err(ScrapeError::SourceUnavailable(503));
    }

    let mut required_fields = Vec::new();
    if shipment
        .get("plzBenoetigt")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        required_fields.push(InputField::DestinationPostcode);
    }
    if shipment
        .get("versandDatumBenoetigt")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        required_fields.push(InputField::ShipmentDate);
    }
    if !required_fields.is_empty() {
        return Ok(LookupOutcome::NeedsInput {
            fields: required_fields,
        });
    }

    if let Some(not_found) = shipment.get("sendungNichtGefunden") {
        let is_not_found = [
            "keineDatenVerfuegbar",
            "keineDhlPaketSendung",
            "sendungsnummerUngueltig",
            "sendungsnummerNichtSuchbar",
            "sendungsdatenZuAlt",
        ]
        .iter()
        .any(|field| not_found.get(*field).and_then(Value::as_bool) == Some(true));
        if is_not_found {
            let reason =
                first_string(not_found, &["/fehlertext", "/fehlertextApp"]).map(str::to_owned);
            return Ok(LookupOutcome::NotFound { reason });
        }
    }

    let details = shipment
        .get("sendungsdetails")
        .ok_or_else(|| ScrapeError::SourceChanged("DHL shipment omitted sendungsdetails".into()))?;
    let history = details
        .get("sendungsverlauf")
        .ok_or_else(|| ScrapeError::SourceChanged("DHL shipment omitted sendungsverlauf".into()))?;

    let events = parse_events(history);
    let raw_status = first_string(
        history,
        &["/status", "/kurzStatus", "/statusKurz", "/aktuellerStatus"],
    )
    .map(plain_text)
    .filter(|status| !status.is_empty())
    .or_else(|| events.first().map(|event| event.description.clone()));

    let delivered = details
        .get("istZugestellt")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_return = details
        .get("ruecksendung")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || details
            .get("retoure")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let progress = history.get("fortschritt").and_then(Value::as_i64);
    let status = normalize_dhl_status(raw_status.as_deref(), delivered, is_return, progress);
    let summary = raw_status.clone().or_else(|| {
        first_string(details, &["/status", "/sendungsstatus", "/produktName"]).map(str::to_owned)
    });

    let estimate = dhl_estimate(details);
    let tracking_number = shipment_identity(shipment)
        .unwrap_or(requested_number)
        .to_owned();
    let sender = first_string(
        details,
        &["/versender/name", "/versender", "/absender/name"],
    )
    .map(str::to_owned);

    Ok(LookupOutcome::Found {
        snapshot: Box::new(TrackingSnapshot {
            carrier: Carrier::DhlPaketDe,
            tracking_number,
            raw_status,
            status,
            summary,
            sender,
            estimate,
            international_tracking_url: first_string(
                details,
                &[
                    "/internationalTrackingLink",
                    "/sendungsnummern/internationaleTrackingUrl",
                ],
            )
            .map(str::to_owned),
            events,
            parser_version: DHL_PARSER_VERSION.to_owned(),
        }),
    })
}

fn looks_like_html(body: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&body[..body.len().min(256)]).to_lowercase();
    prefix.contains("<!doctype html") || prefix.contains("<html")
}

fn shipment_identity(shipment: &Value) -> Option<&str> {
    first_string(
        shipment,
        &[
            "/sendungsdetails/sendungsnummern/sendungsnummer",
            "/sendungsinfo/gesuchteSendungsnummer",
            "/id",
        ],
    )
}

fn shipment_matches(shipment: &Value, requested_number: &str) -> bool {
    [
        "/sendungsdetails/sendungsnummern/sendungsnummer",
        "/sendungsinfo/gesuchteSendungsnummer",
        "/id",
    ]
    .iter()
    .filter_map(|path| shipment.pointer(path).and_then(Value::as_str))
    .any(|identity| identity.eq_ignore_ascii_case(requested_number))
}

fn parse_events(history: &Value) -> Vec<TrackingEvent> {
    history
        .get("events")
        .and_then(Value::as_array)
        .map(|events| {
            events
                .iter()
                .filter_map(|event| {
                    let description = plain_text(first_string(
                        event,
                        &[
                            "/status",
                            "/statusBeschreibung",
                            "/ereignis",
                            "/description",
                        ],
                    )?);
                    if description.is_empty() {
                        return None;
                    }
                    let raw_status =
                        first_string(event, &["/statusCode", "/statuscode", "/code", "/status"])
                            .map(plain_text)
                            .filter(|status| !status.is_empty());
                    Some(TrackingEvent {
                        source_id: first_string(event, &["/id", "/eventId"]).map(str::to_owned),
                        status: normalize_dhl_status(Some(&description), false, false, None),
                        raw_status,
                        description,
                        location: first_string(event, &["/ort", "/location", "/standort"])
                            .map(str::to_owned),
                        timestamp: first_string(event, &["/datum", "/timestamp", "/zeitpunkt"])
                            .map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn dhl_estimate(details: &Value) -> Option<DeliveryEstimate> {
    let delivery = details.get("zustellung")?;
    let from =
        first_string(delivery, &["/zustellzeitfensterVon", "/timeframe/from"]).map(str::to_owned);
    let until =
        first_string(delivery, &["/zustellzeitfensterBis", "/timeframe/to"]).map(str::to_owned);
    let date =
        first_string(delivery, &["/zustelltag", "/prognose", "/deliveryDate"]).map(str::to_owned);
    (from.is_some() || until.is_some() || date.is_some()).then_some(DeliveryEstimate {
        from,
        until,
        date,
    })
}

fn normalize_dhl_status(
    raw_status: Option<&str>,
    delivered: bool,
    is_return: bool,
    progress: Option<i64>,
) -> NormalizedStatus {
    if delivered && is_return {
        return NormalizedStatus::Returned;
    }
    if delivered {
        return NormalizedStatus::Delivered;
    }
    if is_return {
        return NormalizedStatus::Returning;
    }

    let text = raw_status.unwrap_or_default().to_lowercase();
    if text.contains("erfolgreich zugestellt")
        || text.contains("wurde zugestellt")
        || text.contains("successfully delivered")
        || text.contains("was delivered")
        || text.contains("delivered to the recipient")
    {
        NormalizedStatus::Delivered
    } else if text.contains("rücksendung")
        || text.contains("zurückgesandt")
        || text.contains("retoure")
        || text.contains("return shipment")
        || text.contains("returned to sender")
        || text.contains("returning to sender")
    {
        NormalizedStatus::Returning
    } else if text.contains("abholbereit")
        || text.contains("zur abholung bereit")
        || (text.contains("filiale") && text.contains("abholen"))
        || text.contains("ready for pickup")
        || text.contains("ready for collection")
        || (text.contains("branch") && text.contains("collect"))
    {
        NormalizedStatus::ReadyForPickup
    } else if text.contains("zustellversuch")
        || text.contains("nicht zugestellt")
        || text.contains("nicht angetroffen")
        || text.contains("delivery attempt")
        || text.contains("could not be delivered")
        || text.contains("recipient was not present")
    {
        NormalizedStatus::DeliveryAttempted
    } else if text.contains("zustellfahrzeug")
        || text.contains("in zustellung")
        || text.contains("zustellung heute")
        || text.contains("delivery vehicle")
        || text.contains("out for delivery")
        || text.contains("delivery today")
    {
        NormalizedStatus::OutForDelivery
    } else if text.contains("beschädigt")
        || text.contains("verzöger")
        || text.contains("problem")
        || text.contains("fehlgeleitet")
        || text.contains("damaged")
        || text.contains("delayed")
        || text.contains("misrouted")
    {
        NormalizedStatus::Exception
    } else if text.contains("elektronisch angekündigt")
        || text.contains("elektronisch angekuendigt")
        || text.contains("auftragsdaten")
        || text.contains("shipment information received")
        || text.contains("electronically announced")
        || text.contains("order data")
    {
        NormalizedStatus::Announced
    } else if progress.is_some_and(|value| value > 0)
        || text.contains("in der filiale eingeliefert")
        || text.contains("an dhl übergeben")
        || text.contains("weitertransport")
        || (text.contains("zielland") && text.contains("eingetroffen"))
        || text.contains("paketzentrum bearbeitet")
        || text.contains("accepted at the branch")
        || text.contains("handed over to dhl")
        || text.contains("onward transport")
        || text.contains("arrived in the destination country")
        || text.contains("processed at the parcel center")
        || text.contains("processed at the parcel centre")
        || text.contains("in transit")
    {
        NormalizedStatus::InTransit
    } else {
        NormalizedStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{DhlPaketSource, parse_search_response};
    use crate::{
        CarrierSource, InputField, LookupOutcome, NormalizedStatus, ScrapeError, TrackingRequest,
    };
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path, query_param},
    };

    #[test]
    fn parses_a_found_shipment_without_positional_script_selectors() {
        let body = br#"{
          "sendungen": [{
            "id": "00340434161094000000",
            "hasCompleteDetails": true,
            "sendungsdetails": {
              "sendungsnummern": {"sendungsnummer": "00340434161094000000"},
              "sendungsverlauf": {
                "status": "The shipment has been loaded onto the delivery vehicle.",
                "fortschritt": 4,
                "events": [{
                  "status": "The shipment has been loaded onto the delivery vehicle.",
                  "datum": "2026-09-11T07:30:00+02:00",
                  "ort": "Berlin"
                }]
              },
              "zustellung": {
                "zustellzeitfensterVon": "2026-09-11T12:00:00+02:00",
                "zustellzeitfensterBis": "2026-09-11T16:00:00+02:00"
              },
              "istZugestellt": false,
              "ruecksendung": false,
              "retoure": false
            },
            "versandDatumBenoetigt": false,
            "paeckchen": false
          }],
          "rateLimited": false
        }"#;

        let LookupOutcome::Found { snapshot } =
            parse_search_response(body, "00340434161094000000").unwrap()
        else {
            panic!("expected a found shipment")
        };
        assert_eq!(snapshot.status, NormalizedStatus::OutForDelivery);
        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].location.as_deref(), Some("Berlin"));
        assert!(snapshot.estimate.is_some());
    }

    #[test]
    fn normalizes_proven_live_event_text_and_removes_html() {
        let body = br#"{
          "sendungen": [{
            "id": "CY000000000DE",
            "sendungsdetails": {
              "sendungsnummern": {"sendungsnummer": "CY000000000DE"},
              "sendungsverlauf": {
                "status": "Die Sendung wurde zugestellt",
                "events": [
                  {
                    "status": "Die Sendung wurde im Paketzentrum bearbeitet.",
                    "datum": "2026-09-05T05:38:00+02:00"
                  },
                  {
                    "status": "Die Sendung wurde erfolgreich zugestellt. <a href='ignored'>Details</a>",
                    "datum": "2026-09-07T12:03:00+02:00"
                  }
                ]
              },
              "istZugestellt": true
            }
          }]
        }"#;
        let LookupOutcome::Found { snapshot } =
            parse_search_response(body, "CY000000000DE").unwrap()
        else {
            panic!("expected found")
        };
        assert_eq!(snapshot.status, NormalizedStatus::Delivered);
        assert_eq!(snapshot.events[0].status, NormalizedStatus::InTransit);
        assert_eq!(snapshot.events[1].status, NormalizedStatus::Delivered);
        assert_eq!(
            snapshot.events[1].description,
            "Die Sendung wurde erfolgreich zugestellt. Details"
        );
    }

    #[test]
    fn recognizes_postcode_requirement_before_not_found() {
        let body = br#"{
          "sendungen": [{
            "id": "1234567890",
            "hasCompleteDetails": false,
            "plzBenoetigt": true,
            "sendungNichtGefunden": {"keineDatenVerfuegbar": true}
          }]
        }"#;
        assert_eq!(
            parse_search_response(body, "1234567890").unwrap(),
            LookupOutcome::NeedsInput {
                fields: vec![InputField::DestinationPostcode]
            }
        );
    }

    #[test]
    fn recognizes_the_observed_invalid_number_shape() {
        let body = br#"{
          "sendungen": [{
            "id": "0000000000",
            "hasCompleteDetails": true,
            "sendungsdetails": {
              "sendungsnummern": {"sendungsnummer": "0000000000"},
              "sendungsverlauf": {"fortschritt": 0, "events": []}
            },
            "sendungNichtGefunden": {
              "keineDatenVerfuegbar": true,
              "keineDhlPaketSendung": true
            }
          }]
        }"#;
        assert!(matches!(
            parse_search_response(body, "0000000000").unwrap(),
            LookupOutcome::NotFound { .. }
        ));
    }

    #[test]
    fn rejects_html_and_identity_mismatches() {
        assert!(matches!(
            parse_search_response(b"<!doctype html><title>Maintenance</title>", "123"),
            Err(ScrapeError::SourceChanged(_))
        ));
        assert!(matches!(
            parse_search_response(br#"{"sendungen":[{"id":"other"}]}"#, "123"),
            Err(ScrapeError::IdentityMismatch)
        ));
    }

    #[tokio::test]
    async fn performs_config_then_search_with_current_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/config"))
            .and(query_param("domain", "de"))
            .and(query_param("language", "en"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"verfolgenCsrfToken": "fixture-token"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("piececode", "0000000000"))
            .and(query_param("noRedirect", "true"))
            .and(query_param("language", "en"))
            .and(header("verfolgen-csrf-token", "fixture-token"))
            .and(header("verfolgen-wg", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sendungen": [{
                    "id": "0000000000",
                    "sendungNichtGefunden": {"keineDatenVerfuegbar": true}
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let source = DhlPaketSource::with_endpoints(&server.uri(), &server.uri()).unwrap();
        assert!(matches!(
            source.lookup("0000000000").await.unwrap(),
            LookupOutcome::NotFound { .. }
        ));
    }

    #[tokio::test]
    async fn submits_postcode_when_dhl_requests_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/config"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"verfolgenCsrfToken": "fixture-token"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sendungen": [{"id": "1234567890", "plzBenoetigt": true}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/shipment"))
            .and(query_param("language", "en"))
            .and(header("verfolgen-csrf-token", "fixture-token"))
            .and(body_json(serde_json::json!({
                "piececode": "1234567890",
                "zip": "10115",
                "international": false
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sendungen": [{
                    "id": "1234567890",
                    "sendungsdetails": {
                        "sendungsnummern": {"sendungsnummer": "1234567890"},
                        "sendungsverlauf": {
                            "status": "Die Sendung wurde bearbeitet.",
                            "fortschritt": 2,
                            "events": []
                        },
                        "istZugestellt": false
                    }
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let source = DhlPaketSource::with_endpoints(&server.uri(), &server.uri()).unwrap();
        let mut request = TrackingRequest::new("1234567890");
        request.destination_postcode = Some("10115".into());
        let LookupOutcome::Found { snapshot } = source.lookup_request(&request).await.unwrap()
        else {
            panic!("expected a found shipment after postcode submission")
        };
        assert_eq!(snapshot.status, NormalizedStatus::InTransit);
    }

    #[tokio::test]
    async fn submits_shipment_date_when_dhl_requests_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/config"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"verfolgenCsrfToken": "fixture-token"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sendungen": [{"id": "1234567890", "versandDatumBenoetigt": true}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/shipment"))
            .and(query_param("language", "en"))
            .and(header("verfolgen-csrf-token", "fixture-token"))
            .and(body_json(serde_json::json!({
                "piececode": "1234567890",
                "postedDate": "2026-09-10",
                "international": true
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sendungen": [{
                    "id": "1234567890",
                    "sendungsdetails": {
                        "sendungsnummern": {"sendungsnummer": "1234567890"},
                        "sendungsverlauf": {
                            "status": "Die Sendung wurde elektronisch angekündigt.",
                            "fortschritt": 0,
                            "events": []
                        },
                        "istZugestellt": false
                    }
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let source = DhlPaketSource::with_endpoints(&server.uri(), &server.uri()).unwrap();
        let mut request = TrackingRequest::new("1234567890");
        request.shipment_date = Some("2026-09-10".into());
        request.international = true;
        let LookupOutcome::Found { snapshot } = source.lookup_request(&request).await.unwrap()
        else {
            panic!("expected a found shipment after shipment-date submission")
        };
        assert_eq!(snapshot.status, NormalizedStatus::Announced);
    }
}

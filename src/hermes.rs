use async_trait::async_trait;
use reqwest::{
    StatusCode,
    header::{ACCEPT, HeaderName, HeaderValue, ORIGIN, REFERER},
    redirect::Policy,
};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    error::ScrapeError,
    model::{
        Carrier, DeliveryEstimate, LookupOutcome, NormalizedStatus, TrackingEvent, TrackingRequest,
        TrackingSnapshot,
    },
    source::{CarrierSource, first_string, plain_text, retry_after, validate_tracking_number},
};

const DEFAULT_API_BASE: &str = "https://api.my-deliveries.de";
const DEFAULT_REFERER: &str = "https://www.myhermes.de/";
pub const HERMES_PARSER_VERSION: &str = "hermes-de/2026-09-14.2";

#[derive(Clone)]
pub struct HermesSource {
    client: reqwest::Client,
    api_base: String,
    referer: String,
}

impl HermesSource {
    pub fn new() -> Result<Self, ScrapeError> {
        Self::with_endpoints(DEFAULT_API_BASE, DEFAULT_REFERER)
    }

    /// Creates a source with explicit endpoints for deterministic integration tests.
    pub fn with_endpoints(api_base: &str, referer: &str) -> Result<Self, ScrapeError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Walle/", env!("CARGO_PKG_VERSION")))
            .cookie_store(true)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .redirect(Policy::limited(3))
            .build()?;
        Ok(Self {
            client,
            api_base: api_base.trim_end_matches('/').to_owned(),
            referer: referer.to_owned(),
        })
    }

    async fn search(&self, tracking_number: &str) -> Result<LookupOutcome, ScrapeError> {
        let language_header = HeaderName::from_static("x-language");
        let response = self
            .client
            .get(format!(
                "{}/tnt/v2/shipments/search/{tracking_number}",
                self.api_base
            ))
            .header(ACCEPT, HeaderValue::from_static("application/json"))
            .header(language_header, HeaderValue::from_static("en"))
            .header(ORIGIN, self.referer.trim_end_matches('/'))
            .header(REFERER, &self.referer)
            .send()
            .await
            .map_err(|error| ScrapeError::Http(error.without_url()))?;

        match response.status() {
            StatusCode::BAD_REQUEST => {
                return Err(ScrapeError::InvalidInput(
                    "Hermes rejected the tracking-number format".into(),
                ));
            }
            StatusCode::NOT_FOUND => {
                return Ok(LookupOutcome::NotFound { reason: None });
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(ScrapeError::RateLimited {
                    retry_after: retry_after(response.headers()),
                });
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(ScrapeError::AccessDenied);
            }
            status if status.is_server_error() => {
                return Err(ScrapeError::SourceUnavailable(status.as_u16()));
            }
            status if !status.is_success() => {
                return Err(ScrapeError::SourceChanged(format!(
                    "unexpected Hermes HTTP status {}",
                    status.as_u16()
                )));
            }
            _ => {}
        }

        let body = bounded_body(response, 5 * 1_048_576).await?;
        parse_search_response(&body, tracking_number)
    }
}

#[async_trait]
impl CarrierSource for HermesSource {
    async fn lookup_request(
        &self,
        request: &TrackingRequest,
    ) -> Result<LookupOutcome, ScrapeError> {
        let tracking_number = validate_tracking_number(&request.tracking_number)?;
        if !(8..=20).contains(&tracking_number.len()) {
            return Err(ScrapeError::InvalidInput(
                "Hermes accepts 8 to 20 letters or digits".into(),
            ));
        }
        self.search(&tracking_number).await
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
            "Hermes response exceeded the size limit".into(),
        ));
    }
    let body = response.bytes().await?;
    if body.len() > maximum_size {
        return Err(ScrapeError::SourceChanged(
            "Hermes response exceeded the size limit".into(),
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
        if text.contains("access denied") || text.contains("captcha") {
            return Err(ScrapeError::AccessDenied);
        }
        return Err(ScrapeError::SourceChanged(
            "Hermes returned HTML instead of tracking JSON".into(),
        ));
    }

    let root: Value = serde_json::from_slice(body).map_err(|error| {
        ScrapeError::SourceChanged(format!("invalid Hermes tracking JSON: {error}"))
    })?;
    let shipments = root.as_array().ok_or_else(|| {
        ScrapeError::SourceChanged("Hermes response was not a shipment array".into())
    })?;
    if shipments.is_empty() {
        return Ok(LookupOutcome::NotFound { reason: None });
    }

    let shipment = shipments
        .iter()
        .find(|shipment| {
            first_string(shipment, &["/barcode"])
                .is_some_and(|barcode| barcode.eq_ignore_ascii_case(requested_number))
        })
        .ok_or(ScrapeError::IdentityMismatch)?;

    let barcode = first_string(shipment, &["/barcode"])
        .ok_or_else(|| ScrapeError::SourceChanged("Hermes shipment omitted barcode".into()))?;
    let events = parse_events(shipment)?;
    let latest = events.first();
    let raw_status = first_string(shipment, &["/parcelStatus", "/status"])
        .map(plain_text)
        .filter(|status| !status.is_empty())
        .or_else(|| latest.and_then(|event| event.raw_status.clone()));
    let summary = first_string(
        shipment,
        &["/headlineText", "/infoText", "/statusText", "/description"],
    )
    .map(plain_text)
    .filter(|summary| !summary.is_empty())
    .or_else(|| latest.map(|event| event.description.clone()));

    let status = normalize_hermes_status(
        raw_status.as_deref(),
        summary.as_deref(),
        None,
        first_string(shipment, &["/parcelAttributes/directionEnum"]),
    );

    Ok(LookupOutcome::Found {
        snapshot: Box::new(TrackingSnapshot {
            carrier: Carrier::HermesDe,
            tracking_number: barcode.to_owned(),
            raw_status,
            status,
            summary,
            sender: first_string(shipment, &["/atg/companyName", "/sender/name"])
                .map(str::to_owned),
            estimate: hermes_estimate(shipment),
            international_tracking_url: first_string(
                shipment,
                &["/viewParameters/internationalTrackingLink"],
            )
            .map(str::to_owned),
            events,
            parser_version: HERMES_PARSER_VERSION.to_owned(),
        }),
    })
}

fn looks_like_html(body: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&body[..body.len().min(256)]).to_lowercase();
    prefix.contains("<!doctype html") || prefix.contains("<html")
}

fn parse_events(shipment: &Value) -> Result<Vec<TrackingEvent>, ScrapeError> {
    let Some(progress) = shipment.get("parcelProgress") else {
        return Err(ScrapeError::SourceChanged(
            "Hermes shipment omitted parcelProgress".into(),
        ));
    };
    let progress = progress.as_array().ok_or_else(|| {
        ScrapeError::SourceChanged("Hermes parcelProgress was not an array".into())
    })?;

    let mut events = progress
        .iter()
        .filter_map(|event| {
            let description = plain_text(first_string(event, &["/historyText", "/description"])?);
            if description.is_empty() {
                return None;
            }
            let raw_status = first_string(event, &["/parcelStatus", "/status"])
                .map(plain_text)
                .filter(|status| !status.is_empty());
            Some(TrackingEvent {
                source_id: first_string(event, &["/id", "/eventId"]).map(str::to_owned),
                status: normalize_hermes_status(
                    raw_status.as_deref(),
                    Some(&description),
                    first_string(event, &["/status"]),
                    None,
                ),
                raw_status,
                description,
                location: first_string(event, &["/location", "/locationText", "/depot"])
                    .map(str::to_owned),
                timestamp: first_string(event, &["/timestamp"]).map(str::to_owned),
            })
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| compare_timestamps_descending(&left.timestamp, &right.timestamp));
    Ok(events)
}

fn compare_timestamps_descending(
    left: &Option<String>,
    right: &Option<String>,
) -> std::cmp::Ordering {
    let parsed_left = left
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok());
    let parsed_right = right
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok());
    match (parsed_left, parsed_right) {
        (Some(left), Some(right)) => right.cmp(&left),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => right.cmp(left),
    }
}

fn hermes_estimate(shipment: &Value) -> Option<DeliveryEstimate> {
    let from = first_string(
        shipment,
        &[
            "/deliveryForecast/timeframe/from",
            "/viewParameters/deliveryTimeframe/from",
            "/livetrackingOptions/timeframe/from",
        ],
    )
    .map(str::to_owned);
    let until = first_string(
        shipment,
        &[
            "/deliveryForecast/timeframe/to",
            "/viewParameters/deliveryTimeframe/to",
            "/livetrackingOptions/timeframe/to",
        ],
    )
    .map(str::to_owned);
    let date = first_string(
        shipment,
        &["/deliveryForecast/date", "/viewParameters/deliveryDate"],
    )
    .map(str::to_owned);
    (from.is_some() || until.is_some() || date.is_some()).then_some(DeliveryEstimate {
        from,
        until,
        date,
    })
}

fn normalize_hermes_status(
    raw_status: Option<&str>,
    summary: Option<&str>,
    severity: Option<&str>,
    direction: Option<&str>,
) -> NormalizedStatus {
    let code = raw_status.unwrap_or_default().to_ascii_uppercase();
    let direction = direction.unwrap_or_default().to_ascii_uppercase();
    let text = summary.unwrap_or_default().to_lowercase();

    if matches!(
        code.as_str(),
        "RETURN_DELIVERED_TO_SENDER" | "RETOURE_DELIVERED"
    ) {
        NormalizedStatus::Returned
    } else if code.contains("RETURN_TO_SENDER") || direction == "RETURN" {
        NormalizedStatus::Returning
    } else if matches!(
        code.as_str(),
        "DELIVERED" | "DELIVERED_TO_RECIPIENT" | "PARCEL_DELIVERED" | "COLLECTED"
    ) || (severity == Some("FINISHED")
        && (text.contains("zugestellt")
            || text.contains("abgeholt")
            || text.contains("delivered")
            || text.contains("collected")))
    {
        NormalizedStatus::Delivered
    } else if code.contains("READY_FOR_PICKUP")
        || text.contains("abholbereit")
        || text.contains("zur abholung bereit")
        || text.contains("ready for pickup")
        || text.contains("ready for collection")
    {
        NormalizedStatus::ReadyForPickup
    } else if code.contains("DELIVERY_ATTEMPT")
        || text.contains("zustellversuch")
        || text.contains("nicht angetroffen")
        || text.contains("delivery attempt")
        || text.contains("recipient was not present")
    {
        NormalizedStatus::DeliveryAttempted
    } else if code.contains("OUT_FOR_DELIVERY")
        || text.contains("in zustellung")
        || text.contains("zustellfahrzeug")
        || text.contains("out for delivery")
        || text.contains("delivery vehicle")
    {
        NormalizedStatus::OutForDelivery
    } else if severity == Some("UNHAPPY")
        || code.contains("EXCEPTION")
        || text.contains("beschädigt")
        || text.contains("verzöger")
        || text.contains("problem")
        || text.contains("damaged")
        || text.contains("delayed")
    {
        NormalizedStatus::Exception
    } else if code == "ANNOUNCED"
        || text.contains("angekündigt")
        || text.contains("shipment information")
        || text.contains("electronically announced")
    {
        NormalizedStatus::Announced
    } else if code.contains("CANCELLED") || code.contains("CANCELED") {
        NormalizedStatus::Cancelled
    } else if code.contains("IN_TRANSIT")
        || text.contains("unterwegs")
        || text.contains("transportiert")
        || text.contains("verteilzentrum")
        || text.contains("logistikzentrum")
        || text.contains("in transit")
        || text.contains("on its way")
        || text.contains("sorting center")
        || text.contains("sorting centre")
        || text.contains("logistics center")
        || text.contains("logistics centre")
    {
        NormalizedStatus::InTransit
    } else {
        NormalizedStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{HermesSource, parse_search_response};
    use crate::{CarrierSource, LookupOutcome, NormalizedStatus, ScrapeError};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[test]
    fn parses_current_bundle_field_names() {
        let body = r#"[{
          "barcode": "HERMES12345",
          "atg": {"companyName": "Example Shop"},
          "parcelAttributes": {"directionEnum": "DELIVERY"},
          "parcelProgress": [
            {
              "parcelStatus": "ANNOUNCED",
              "status": "INFORMATION",
              "timestamp": "2026-09-10T11:00:00+02:00",
              "historyText": "The shipment information was received."
            },
            {
              "parcelStatus": "OUT_FOR_DELIVERY",
              "status": "INFORMATION",
              "timestamp": "2026-09-11T08:00:00+02:00",
              "historyText": "The shipment is out for delivery."
            }
          ],
          "viewParameters": {
            "internationalTrackingLink": "https://example.invalid/partner"
          }
        }]"#
        .as_bytes();

        let LookupOutcome::Found { snapshot } = parse_search_response(body, "HERMES12345").unwrap()
        else {
            panic!("expected found")
        };
        assert_eq!(snapshot.status, NormalizedStatus::OutForDelivery);
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.sender.as_deref(), Some("Example Shop"));
    }

    #[test]
    fn recognizes_returns_and_empty_results() {
        let returned = r#"[{
          "barcode":"HERMES12345",
          "parcelAttributes":{"directionEnum":"RETURN"},
          "parcelProgress":[{
            "parcelStatus":"RETURN_DELIVERED_TO_SENDER",
            "status":"FINISHED",
            "timestamp":"2026-09-11T10:00:00+02:00",
            "historyText":"Die Retoure wurde zugestellt."
          }]
        }]"#
        .as_bytes();
        let LookupOutcome::Found { snapshot } =
            parse_search_response(returned, "HERMES12345").unwrap()
        else {
            panic!("expected found")
        };
        assert_eq!(snapshot.status, NormalizedStatus::Returned);
        assert!(matches!(
            parse_search_response(b"[]", "HERMES12345").unwrap(),
            LookupOutcome::NotFound { .. }
        ));
    }

    #[test]
    fn normalizes_english_history_text_without_a_known_status_code() {
        let body = br#"[{
          "barcode":"HERMES12345",
          "parcelProgress":[{
            "parcelStatus":"STATUS_UPDATE",
            "status":"INFORMATION",
            "timestamp":"2026-09-11T09:30:00Z",
            "historyText":"The shipment is out for delivery."
          }]
        }]"#;
        let LookupOutcome::Found { snapshot } = parse_search_response(body, "HERMES12345").unwrap()
        else {
            panic!("expected found")
        };
        assert_eq!(snapshot.status, NormalizedStatus::OutForDelivery);
    }

    #[test]
    fn chooses_the_latest_event_by_instant_across_offsets() {
        let body = br#"[{
          "barcode":"HERMES12345",
          "parcelProgress":[
            {
              "parcelStatus":"IN_TRANSIT",
              "status":"INFORMATION",
              "timestamp":"2026-09-11T10:00:00+02:00",
              "historyText":"Earlier instant"
            },
            {
              "parcelStatus":"OUT_FOR_DELIVERY",
              "status":"INFORMATION",
              "timestamp":"2026-09-11T09:30:00Z",
              "historyText":"Later instant"
            }
          ]
        }]"#;
        let LookupOutcome::Found { snapshot } = parse_search_response(body, "HERMES12345").unwrap()
        else {
            panic!("expected found")
        };
        assert_eq!(snapshot.summary.as_deref(), Some("Later instant"));
        assert_eq!(snapshot.status, NormalizedStatus::OutForDelivery);
    }

    #[test]
    fn rejects_unrecognized_or_mismatched_responses() {
        assert!(matches!(
            parse_search_response(br#"{"error":"changed"}"#, "HERMES12345"),
            Err(ScrapeError::SourceChanged(_))
        ));
        assert!(matches!(
            parse_search_response(
                br#"[{"barcode":"OTHER123","parcelProgress":[]}]"#,
                "HERMES12345"
            ),
            Err(ScrapeError::IdentityMismatch)
        ));
    }

    #[tokio::test]
    async fn calls_the_current_search_route_with_language_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tnt/v2/shipments/search/HERMES12345"))
            .and(header("x-language", "en"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .expect(1)
            .mount(&server)
            .await;

        let source = HermesSource::with_endpoints(&server.uri(), &server.uri()).unwrap();
        assert!(matches!(
            source.lookup("HERMES12345").await.unwrap(),
            LookupOutcome::NotFound { .. }
        ));
    }
}

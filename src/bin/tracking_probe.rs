use std::io::{self, Write};

use serde_json::json;
use walle_tracking::{
    CarrierSource, DhlPaketSource, HermesSource, InputField, LookupOutcome, ScrapeError,
    TrackingRequest,
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Lookup failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), ScrapeError> {
    let carrier = std::env::args()
        .nth(1)
        .ok_or_else(|| ScrapeError::InvalidInput("usage: tracking-probe <dhl|hermes>".into()))?;

    print!("Tracking number (redacted in probe output): ");
    io::stdout()
        .flush()
        .map_err(|error| ScrapeError::InvalidInput(error.to_string()))?;
    let tracking_number = read_line()?;
    let mut request = TrackingRequest::new(&tracking_number);
    let mut outcome = lookup(&carrier, &request).await?;

    if let LookupOutcome::NeedsInput { fields } = &outcome {
        if fields.contains(&InputField::DestinationPostcode) {
            print!("Destination postcode: ");
            io::stdout()
                .flush()
                .map_err(|error| ScrapeError::InvalidInput(error.to_string()))?;
            request.destination_postcode = Some(read_line()?);
        }
        if fields.contains(&InputField::ShipmentDate) {
            print!("Shipment date (YYYY-MM-DD): ");
            io::stdout()
                .flush()
                .map_err(|error| ScrapeError::InvalidInput(error.to_string()))?;
            request.shipment_date = Some(read_line()?);
        }
        print!("International shipment? [y/N]: ");
        io::stdout()
            .flush()
            .map_err(|error| ScrapeError::InvalidInput(error.to_string()))?;
        request.international = matches!(read_line()?.to_ascii_lowercase().as_str(), "y" | "yes");
        outcome = lookup(&carrier, &request).await?;
    }

    let mut output = match outcome {
        LookupOutcome::Found { snapshot } => json!({
            "outcome": "found",
            "carrier": snapshot.carrier,
            "tracking_number": redact(&tracking_number),
            "status": snapshot.status,
            "raw_status": snapshot.raw_status,
            "summary": snapshot.summary,
            "sender": snapshot.sender,
            "estimate": snapshot.estimate,
            "international_tracking_url": snapshot.international_tracking_url,
            "events": snapshot.events,
            "parser_version": snapshot.parser_version,
        }),
        LookupOutcome::NotFound { reason } => json!({
            "outcome": "not_found",
            "tracking_number": redact(&tracking_number),
            "reason": reason,
        }),
        LookupOutcome::NeedsInput { fields } => json!({
            "outcome": "needs_input",
            "tracking_number": redact(&tracking_number),
            "fields": fields,
        }),
    };
    redact_json(&mut output, &tracking_number);

    println!(
        "{}",
        serde_json::to_string_pretty(&output)
            .map_err(|error| ScrapeError::SourceChanged(error.to_string()))?
    );
    Ok(())
}

async fn lookup(carrier: &str, request: &TrackingRequest) -> Result<LookupOutcome, ScrapeError> {
    match carrier.to_ascii_lowercase().as_str() {
        "dhl" | "dhl-paket" => DhlPaketSource::new()?.lookup_request(request).await,
        "hermes" => HermesSource::new()?.lookup_request(request).await,
        _ => Err(ScrapeError::InvalidInput(
            "carrier must be dhl or hermes".into(),
        )),
    }
}

fn read_line() -> Result<String, ScrapeError> {
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| ScrapeError::InvalidInput(error.to_string()))?;
    Ok(value.trim().to_owned())
}

fn redact(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    if characters.len() <= 4 {
        return "****".into();
    }
    let suffix = characters[characters.len() - 4..]
        .iter()
        .collect::<String>();
    format!("{}{}", "*".repeat(characters.len() - 4), suffix)
}

fn redact_sensitive_text(value: &str, secret: &str) -> String {
    let replacement = redact(secret);
    [
        secret.to_owned(),
        secret.to_ascii_lowercase(),
        secret.to_ascii_uppercase(),
    ]
    .into_iter()
    .fold(value.to_owned(), |text, candidate| {
        text.replace(&candidate, &replacement)
    })
}

fn redact_tracking_query_values(value: &str) -> String {
    let mut output = value.to_owned();
    for marker in ["tracking-number=", "tracking-id=", "piececode="] {
        let mut cursor = 0;
        loop {
            let lowercase_tail = output[cursor..].to_ascii_lowercase();
            let Some(relative_marker) = lowercase_tail.find(marker) else {
                break;
            };
            let value_start = cursor + relative_marker + marker.len();
            let value_end = value_start
                + output[value_start..]
                    .find(|character: char| {
                        character.is_whitespace()
                            || matches!(character, '&' | '#' | '\'' | '"' | '<' | '>' | '(' | ')')
                    })
                    .unwrap_or(output.len() - value_start);
            if value_start == value_end {
                cursor = value_start;
                continue;
            }
            let replacement = redact(&output[value_start..value_end]);
            output.replace_range(value_start..value_end, &replacement);
            cursor = value_start + replacement.len();
        }
    }
    output
}

fn redact_json(value: &mut serde_json::Value, secret: &str) {
    match value {
        serde_json::Value::String(text) => {
            *text = redact_tracking_query_values(&redact_sensitive_text(text, secret));
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json(item, secret);
            }
        }
        serde_json::Value::Object(fields) => {
            for value in fields.values_mut() {
                redact_json(value, secret);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{redact, redact_json};

    #[test]
    fn redacts_all_but_the_last_four_characters() {
        assert_eq!(redact("1234567890"), "******7890");
        assert_eq!(redact("1234"), "****");
    }

    #[test]
    fn redacts_numbers_inside_nested_carrier_text_and_links() {
        let mut output = serde_json::json!({
            "events": [{
                "description": "https://example.invalid/?tracking-id=CY000000000DE"
            }],
            "raw": "cy227150684de",
            "international_tracking_url": "https://partner.invalid/?tracking-number=PQAFDC9800941780181600L"
        });
        redact_json(&mut output, "CY000000000DE");
        let serialized = serde_json::to_string(&output).unwrap();
        assert!(!serialized.contains("CY000000000DE"));
        assert!(!serialized.contains("cy000000000de"));
        assert!(!serialized.contains("PQAFDC9800941780181600L"));
        assert!(serialized.contains("*********00DE"));
        assert!(serialized.contains("******************600L"));
    }
}

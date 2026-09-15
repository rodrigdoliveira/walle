use async_trait::async_trait;

use crate::{
    error::ScrapeError,
    model::{LookupOutcome, TrackingRequest},
};

#[async_trait]
pub trait CarrierSource: Send + Sync {
    async fn lookup_request(&self, request: &TrackingRequest)
    -> Result<LookupOutcome, ScrapeError>;

    async fn lookup(&self, tracking_number: &str) -> Result<LookupOutcome, ScrapeError> {
        self.lookup_request(&TrackingRequest::new(tracking_number))
            .await
    }
}

pub(crate) fn validate_tracking_number(value: &str) -> Result<String, ScrapeError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(ScrapeError::InvalidInput("the number is empty".into()));
    }
    if normalized.len() > 100 {
        return Err(ScrapeError::InvalidInput("the number is too long".into()));
    }
    if !normalized
        .chars()
        .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(ScrapeError::InvalidInput(
            "only ASCII letters and digits are accepted by these sources".into(),
        ));
    }
    Ok(normalized.to_owned())
}

pub(crate) fn first_string<'a>(value: &'a serde_json::Value, paths: &[&str]) -> Option<&'a str> {
    paths
        .iter()
        .find_map(|path| value.pointer(path).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// Converts the small HTML fragments used by carrier status feeds to safe,
/// readable text before they cross the adapter boundary.
pub(crate) fn plain_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut inside_tag = false;

    for character in value.chars() {
        match character {
            '<' => {
                inside_tag = true;
                if !output.ends_with(char::is_whitespace) {
                    output.push(' ');
                }
            }
            '>' if inside_tag => {
                inside_tag = false;
                if !output.ends_with(char::is_whitespace) {
                    output.push(' ');
                }
            }
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }

    let decoded = output
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<std::time::Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::{plain_text, validate_tracking_number};

    #[test]
    fn validation_trims_and_preserves_leading_zeroes() {
        assert_eq!(validate_tracking_number("  0012AB  ").unwrap(), "0012AB");
    }

    #[test]
    fn validation_rejects_control_and_separator_characters() {
        assert!(validate_tracking_number("12;34").is_err());
        assert!(validate_tracking_number("12\n34").is_err());
    }

    #[test]
    fn carrier_html_is_reduced_to_plain_text() {
        assert_eq!(
            plain_text("Arrived&nbsp;<a href='ignored'>at destination</a> &amp; sorted"),
            "Arrived at destination & sorted"
        );
    }
}

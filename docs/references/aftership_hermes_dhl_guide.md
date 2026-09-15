# Hermes and DHL tracking with the AfterShip API

Examples for **Hermes Germany** and **DHL Paket Germany**, plus the carrier change for **DHL Express**.

**API version:** `2026-07`
**Documentation checked:** September 11, 2026
**Scope:** Retrieve parcel status and shipment events. These examples do not buy labels or book shipments.

> You need your own AfterShip API key and real tracking numbers you are authorized to use. No authenticated shipment requests were executed while preparing this guide. Sample values are placeholders, not working test shipments.

**Validation:** The curl/jq workflow was exercised with offline fixtures, all code blocks were syntax-checked, and 12 Python tests passed using simulated responses. This is not a live integration test.

## 1. Choose the correct carrier

Both carriers use the same AfterShip endpoints. The `slug` selects the service. AfterShip lists these standard slugs alongside optional account-connected alternatives. This guide uses the standard slugs. [Supported carriers][carriers]

| Service | `slug` used in this guide |
| --- | --- |
| Hermes Germany | `hermes-de` |
| DHL Paket Germany / Deutsche Post DHL | `dhl-germany` |
| DHL Express | `dhl` |

Do not use `dhl` for every DHL parcel: select the actual service. Do not substitute `dhl-germany-api` or `hermes-de-ftp` unless you have set up the corresponding carrier connection. [Supported carriers][carriers]

The workflow is: **register the shipment → save its AfterShip ID → retrieve updates using that ID**, or receive updates through a webhook. [API quick start][quickstart]

## 2. Set up authentication

In your AfterShip account, open **API keys** and create a key using the plain **API Key** authentication method. The examples send it in the `as-api-key` header; keys configured for AES or RSA need additional signing and are outside this guide. [Authentication][auth]

Confirm your account has Tracking API access and a suitable shipment allowance before registering parcels. Do not assume that possessing an API key means unlimited usage.

The shell examples require **Bash, curl, and jq**. Run them in sequence in the same Bash session, or combine them into a shell script. `set -e` stops execution on an error; resolve the error before continuing. Response files may contain shipment information, so store them privately.

```bash
set -euo pipefail

export AFTERSHIP_BASE_URL='https://api.aftership.com/tracking/2026-07'

# Enter the key interactively instead of putting it in shell history.
read -r -s -p 'AfterShip API key: ' AFTERSHIP_API_KEY
printf '\n'
export AFTERSHIP_API_KEY

# Replace these placeholders with your own tracking numbers.
export HERMES_TRACKING_NUMBER='REPLACE_WITH_REAL_HERMES_NUMBER'
export DHL_TRACKING_NUMBER='REPLACE_WITH_REAL_DHL_PAKET_NUMBER'
```

Keep the key on your backend, not in browser JavaScript, a mobile app bundle, a public repository, or this Markdown file.

## 3. Register one Hermes shipment and one DHL shipment

Use `POST /trackings`. The JSON request has `tracking_number` and `slug` at the top level—do not add a `tracking` wrapper. The `2026-07` API returns the created shipment directly under `data`, so its ID is `data.id`. [Create a tracking][create] · [Response envelope][envelope] · [Official response model][create-model] · [Official response parser][response-parser]

Registration adds a tracking to your AfterShip account; it is not a read-only operation. Run it once for each shipment. For an already registered shipment, use the lookup in section 5 instead.

### Hermes Germany

```bash
curl --fail-with-body --silent --show-error \
  --connect-timeout 10 --max-time 30 \
  --request POST "${AFTERSHIP_BASE_URL}/trackings" \
  --header "as-api-key: ${AFTERSHIP_API_KEY}" \
  --header 'Content-Type: application/json' \
  --data "$(jq -n --arg number "$HERMES_TRACKING_NUMBER" \
    '{tracking_number: $number, slug: "hermes-de"}')" \
  --output hermes-created.json

HERMES_ID="$(jq -er '.data.id | select(type == "string" and length > 0)' hermes-created.json)"
export HERMES_ID
printf 'Hermes AfterShip ID: %s\n' "$HERMES_ID"
```

### DHL Paket Germany

```bash
curl --fail-with-body --silent --show-error \
  --connect-timeout 10 --max-time 30 \
  --request POST "${AFTERSHIP_BASE_URL}/trackings" \
  --header "as-api-key: ${AFTERSHIP_API_KEY}" \
  --header 'Content-Type: application/json' \
  --data "$(jq -n --arg number "$DHL_TRACKING_NUMBER" \
    '{tracking_number: $number, slug: "dhl-germany"}')" \
  --output dhl-created.json

DHL_ID="$(jq -er '.data.id | select(type == "string" and length > 0)' dhl-created.json)"
export DHL_ID
printf 'DHL AfterShip ID: %s\n' "$DHL_ID"
```

`jq --arg` keeps the tracking number as a JSON string. Do not convert tracking numbers to integers: leading zeros must be preserved.

### DHL Express variation

For an Express shipment, use the same POST endpoint with the following body instead. Do not reuse a DHL Paket tracking number for this example. [Supported carriers][carriers]

```json
{
  "tracking_number": "YOUR_DHL_EXPRESS_TRACKING_NUMBER",
  "slug": "dhl"
}
```

### Additional tracking fields

Some carrier lookups need extra information. Supply the real `destination_postal_code` when required for your shipment; do not assume a postcode is mandatory for every Hermes or DHL request. The create schema and additional-fields reference document this field. [Create a tracking][create] · [Additional tracking fields][extra-fields]

For example, a DHL request with a recipient postcode would have this structure:

```json
{
  "tracking_number": "YOUR_DHL_PAKET_TRACKING_NUMBER",
  "slug": "dhl-germany",
  "destination_postal_code": "ACTUAL_RECIPIENT_POSTCODE"
}
```

## 4. Retrieve status and shipment events

Use `GET /trackings/{id}`, replacing `{id}` with the **AfterShip ID**, not the carrier's tracking number. For a new terminal session, restore the saved IDs from your files or database before running these commands. [Get a tracking by ID][get]

```bash
curl --fail-with-body --silent --show-error \
  --connect-timeout 10 --max-time 30 \
  "${AFTERSHIP_BASE_URL}/trackings/${HERMES_ID}" \
  --header "as-api-key: ${AFTERSHIP_API_KEY}" \
  --header 'Content-Type: application/json' \
  --output hermes-latest.json

curl --fail-with-body --silent --show-error \
  --connect-timeout 10 --max-time 30 \
  "${AFTERSHIP_BASE_URL}/trackings/${DHL_ID}" \
  --header "as-api-key: ${AFTERSHIP_API_KEY}" \
  --header 'Content-Type: application/json' \
  --output dhl-latest.json
```

The shipment's `tag` is its normalized delivery status; `subtag` and `subtag_message` give more detail. `checkpoints` contains shipment events, with fields such as `checkpoint_time`, `message`, and `location`. Do not interpret an event's `created_at` as its carrier event time. [Tracking model][tracking-model]

Print the same useful fields for both shipments:

```bash
jq '.data | {
  aftership_id: .id,
  carrier: .slug,
  tracking_number: .tracking_number,
  status: .tag,
  status_detail: .subtag_message,
  events: ((.checkpoints // []) | map({
    time: .checkpoint_time,
    status: .tag,
    message: .message,
    location: .location
  }))
}' hermes-latest.json dhl-latest.json
```

The following is **illustrative output from the jq transformation**, not a real API response or an actual shipment:

```json
{
  "aftership_id": "EXAMPLE_AFTERSHIP_ID",
  "carrier": "hermes-de",
  "tracking_number": "YOUR_HERMES_TRACKING_NUMBER",
  "status": "InTransit",
  "status_detail": "In transit",
  "events": [
    {
      "time": "2026-09-10T14:30:00+02:00",
      "status": "InTransit",
      "message": "Illustrative carrier scan message",
      "location": "Illustrative parcel facility"
    }
  ]
}
```

Handle missing or empty checkpoints rather than assuming that registration immediately provides shipment events. The quick start separates registration from receiving updates. [API quick start][quickstart]

## 5. Find existing shipments or recover a lost AfterShip ID

AfterShip reports duplicate registration as **HTTP 400 with `meta.code` 4003**, not necessarily HTTP 409. Retrieve the existing tracking instead of deleting and recreating it. [Request errors][errors]

`GET /trackings` accepts `tracking_numbers` and `slug` filters, including comma-separated values. Its list of shipments is under `data.trackings`; this differs from a single-shipment response, where the shipment is directly under `data`. [Get trackings][list] · [Official list response model][list-model]

This request looks for the two shipments already registered in your account:

```bash
curl --fail-with-body --silent --show-error \
  --connect-timeout 10 --max-time 30 \
  --get "${AFTERSHIP_BASE_URL}/trackings" \
  --header "as-api-key: ${AFTERSHIP_API_KEY}" \
  --header 'Content-Type: application/json' \
  --data-urlencode "tracking_numbers=${HERMES_TRACKING_NUMBER},${DHL_TRACKING_NUMBER}" \
  --data-urlencode 'slug=hermes-de,dhl-germany' \
  --output existing-trackings.json

jq '.data.trackings[] | {id, slug, tracking_number, tag}' existing-trackings.json
```

Match **both** the returned carrier and tracking number before storing an ID. This is a search of account records, not a substitute for registering a new parcel. For broader searches, implement the endpoint's cursor pagination. [Get trackings][list]

## 6. Reusable Python example for both carriers

This alternative uses **Python 3.10+ and only the standard library**; no AfterShip SDK or third-party Python package is needed. Save the following code as `aftership_example.py`.

It registers each shipment, recovers an existing record on error `4003`, fetches both statuses, and prints a JSON array. It uses 30-second request timeouts and does not blindly retry POST requests. The API fields, endpoints, response layout, and duplicate code correspond to the references above.

```python
#!/usr/bin/env python3
"""Track Hermes Germany and DHL Paket Germany via AfterShip (Python 3.10+)."""

from __future__ import annotations

import json
import os
import sys
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen

BASE_URL = "https://api.aftership.com/tracking/2026-07"


class AfterShipError(RuntimeError):
    def __init__(self, status: int, meta_code: Any, message: str) -> None:
        self.status = status
        self.meta_code = str(meta_code)
        super().__init__(f"HTTP {status}; AfterShip {meta_code}: {message}")


class AfterShipClient:
    def __init__(self, api_key: str) -> None:
        if not api_key.strip():
            raise ValueError("AFTERSHIP_API_KEY must not be empty.")
        self.api_key = api_key.strip()

    def request(
        self,
        method: str,
        path: str,
        *,
        payload: dict[str, Any] | None = None,
        query: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        url = BASE_URL + path
        if query:
            url += "?" + urlencode(query)
        body = None if payload is None else json.dumps(payload).encode("utf-8")
        request = Request(
            url,
            data=body,
            method=method,
            headers={
                "as-api-key": self.api_key,
                "Content-Type": "application/json",
                "Accept": "application/json",
            },
        )
        try:
            with urlopen(request, timeout=30) as response:
                raw = response.read()
        except HTTPError as exc:
            try:
                error = json.loads(exc.read())
                meta = error.get("meta", {}) if isinstance(error, dict) else {}
                if not isinstance(meta, dict):
                    meta = {}
            except (ValueError, UnicodeDecodeError):
                meta = {}
            message = str(meta.get("message") or exc.reason)
            message = message.replace(self.api_key, "[REDACTED]")
            raise AfterShipError(
                exc.code, meta.get("code", exc.code), message
            ) from exc
        except (URLError, TimeoutError, OSError) as exc:
            raise RuntimeError(
                f"{method} request failed at the network layer. "
                "After an uncertain POST, look up the shipment before retrying."
            ) from exc

        try:
            envelope = json.loads(raw)
        except (ValueError, UnicodeDecodeError) as exc:
            raise RuntimeError("AfterShip returned a non-JSON response.") from exc
        if not isinstance(envelope, dict) or not isinstance(envelope.get("data"), dict):
            raise RuntimeError("Unexpected response: expected a JSON data object.")
        return envelope["data"]

    def create_or_get(
        self, number: str, slug: str, postal_code: str | None = None
    ) -> dict[str, Any]:
        number = number.strip()
        if not number:
            raise ValueError("Tracking number must not be empty.")
        payload: dict[str, Any] = {"tracking_number": number, "slug": slug}
        if postal_code and postal_code.strip():
            payload["destination_postal_code"] = postal_code.strip()
        try:
            return self.request("POST", "/trackings", payload=payload)
        except AfterShipError as exc:
            if exc.meta_code != "4003":
                raise

        # A duplicate is not a reason to delete/recreate the tracking.
        data = self.request(
            "GET", "/trackings", query={"tracking_numbers": number, "slug": slug}
        )
        matches = [
            item
            for item in (data.get("trackings") or [])
            if isinstance(item, dict)
            and item.get("tracking_number") == number
            and item.get("slug") == slug
        ]
        if len(matches) != 1:
            raise RuntimeError(
                "Duplicate reported, but lookup did not return exactly one match. "
                "Check the AfterShip dashboard and use the correct stored ID."
            )
        return matches[0]

    def get(self, tracking_id: str) -> dict[str, Any]:
        if not tracking_id:
            raise ValueError("An AfterShip tracking ID is required.")
        return self.request("GET", f"/trackings/{quote(tracking_id, safe='')}")


def main() -> int:
    names = ("AFTERSHIP_API_KEY", "HERMES_TRACKING_NUMBER", "DHL_TRACKING_NUMBER")
    missing = [name for name in names if not os.environ.get(name, "").strip()]
    if missing:
        print("Set these environment variables: " + ", ".join(missing), file=sys.stderr)
        return 2

    client = AfterShipClient(os.environ["AFTERSHIP_API_KEY"])
    shipments = (
        ("hermes-de", "HERMES_TRACKING_NUMBER", "HERMES_POSTAL_CODE"),
        ("dhl-germany", "DHL_TRACKING_NUMBER", "DHL_POSTAL_CODE"),
    )
    results: list[dict[str, Any]] = []
    failed = False
    for slug, number_env, postal_env in shipments:
        try:
            registered = client.create_or_get(
                os.environ[number_env], slug, os.environ.get(postal_env)
            )
            tracking_id = registered.get("id")
            if not isinstance(tracking_id, str) or not tracking_id:
                raise RuntimeError("The tracking response did not contain a valid id.")
            current = client.get(tracking_id)
            results.append({
                "aftership_id": current.get("id"),
                "carrier": current.get("slug"),
                "tracking_number": current.get("tracking_number"),
                "status": current.get("tag"),
                "status_detail": current.get("subtag_message"),
                "checkpoints": current.get("checkpoints") or [],
            })
        except (RuntimeError, ValueError) as exc:
            failed = True
            print(f"{slug}: {exc}", file=sys.stderr)

    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
```

Run it with the environment variables from section 2:

```bash
python3 aftership_example.py > tracking-results.json
```

The program writes successful results to standard output and errors to standard error. Exit code `0` means both lookups succeeded; `1` means at least one failed; `2` means configuration is missing. Inspect errors before treating `tracking-results.json` as a complete result.

When a real recipient postcode is needed, set the corresponding `HERMES_POSTAL_CODE` or `DHL_POSTAL_CODE` environment variable before running the script. The client adds a nonempty value as `destination_postal_code`.

For DHL Express, change the Python shipment tuple from `("dhl-germany", "DHL_TRACKING_NUMBER", "DHL_POSTAL_CODE")` to `("dhl", "DHL_TRACKING_NUMBER", "DHL_POSTAL_CODE")`, and supply an Express tracking number.

**For subsequent refreshes, use the saved ID directly** rather than repeatedly running the registration demo:

```python
import os
from aftership_example import AfterShipClient

client = AfterShipClient(os.environ["AFTERSHIP_API_KEY"])
current = client.get("YOUR_SAVED_AFTERSHIP_ID")
print(current.get("tag"))
```

## 7. Troubleshooting and production use

The HTTP and AfterShip codes below come from the request-error reference. [Request errors][errors]

| Error | Action |
| --- | --- |
| HTTP `401` | Check the API key and authentication method. |
| HTTP `403` | Check account permissions and whether the requested access is allowed. |
| HTTP `400`, `meta.code = 4003` | Tracking already exists. Recover its ID with section 5. |
| HTTP `400`, `meta.code = 4011` | Supply the additional field named in the error using genuine shipment details. |
| HTTP `404`, `meta.code = 4004` | Confirm the saved AfterShip ID and the account used to create the tracking. |
| HTTP `429` | Stop the burst of requests and retry later according to rate-limit information. |
| HTTP `5xx` or network timeout | A request may have an uncertain outcome. Use bounded backoff for reads; reconcile a POST through lookup before resubmitting it. |

With the curl commands above, an API error body is saved in the specified output JSON file. Inspect its `meta` object for the diagnostic code and message.

**Rate limits and retries.** Use the response headers `X-RateLimit-Limit`, `X-RateLimit-Remaining`, and `X-RateLimit-Reset` to regulate traffic. Limits differ by endpoint. The examples deliberately make no automatic retries; add a bounded retry policy and caching before using them in a service. [Rate limits][rate-limit]

**Webhooks.** For ongoing tracking, consider receiving updates instead of polling on every page view. Configure a webhook in AfterShip, verify `aftership-hmac-sha256` against the Base64-encoded HMAC-SHA256 of the **raw request body** using your webhook secret, and make processing idempotent. Keep the webhook secret separate from the API key. Signature validation alone does not replace duplicate/replay handling. [API quick start][quickstart] · [Webhook signature][webhook-signature]

**Storage and privacy.** Store your internal shipment reference alongside its carrier, carrier tracking number, and AfterShip ID. Keep raw responses and tracking numbers out of public logs; request only shipment data you are authorized to access. Keep credentials server-side. Do not schedule repeated registration calls as your polling mechanism.

## Official references

All references were checked on September 11, 2026. Examples are pinned to `2026-07`; recheck versioning, carrier support, and account requirements when implementing them later.

| Reference | What it supports |
| --- | --- |
| [API quick start][quickstart] | API base URL and create/read/webhook workflow. |
| [Authentication][auth] | API key creation and `as-api-key` authentication. |
| [Supported carriers][carriers] | Hermes and DHL carrier slugs and connected alternatives. |
| [Create a tracking][create] | POST request fields. |
| [Get a tracking by ID][get] | Reading a shipment by its AfterShip ID. |
| [Get trackings][list] | Filters and cursor-based listing. |
| [Response envelope][envelope] | `meta` and `data` response structure. |
| [Tracking model][tracking-model] | Status and checkpoint fields. |
| [Additional tracking fields][extra-fields] | Extra shipment information, including recipient postcode. |
| [Request errors][errors] | HTTP and `meta.code` error meanings. |
| [Rate limits][rate-limit] | Endpoint-specific limits and response headers. |
| [Webhook signature][webhook-signature] | Webhook signature header and verification algorithm. |
| [Official create response model][create-model], [response parser][response-parser], and [list response model][list-model] | Cross-check of single-tracking and list JSON layouts in AfterShip's `2026-07` SDK source. |

[quickstart]: https://www.aftership.com/docs/tracking/quickstart/api-quick-start
[auth]: https://www.aftership.com/docs/tracking/quickstart/authentication
[carriers]: https://www.aftership.com/docs/tracking/others/supported-couriers
[create]: https://www.aftership.com/docs/tracking/sxafu5cay1usl-create-a-tracking
[get]: https://www.aftership.com/docs/tracking/bcb1azgtk9n6r-get-a-tracking-by-id
[list]: https://www.aftership.com/docs/tracking/jh865r66gc6hi-get-trackings
[envelope]: https://www.aftership.com/docs/tracking/quickstart/body-envelope
[tracking-model]: https://www.aftership.com/docs/tracking/model/tracking
[extra-fields]: https://www.aftership.com/docs/tracking/enum/additional-tracking-fields
[errors]: https://www.aftership.com/docs/tracking/quickstart/request-errors
[rate-limit]: https://www.aftership.com/docs/tracking/quickstart/rate-limit
[webhook-signature]: https://www.aftership.com/docs/tracking/webhook/webhook-signature
[create-model]: https://raw.githubusercontent.com/AfterShip/tracking-sdk-python/2026-07/tracking/models/create_tracking_response.py
[response-parser]: https://raw.githubusercontent.com/AfterShip/tracking-sdk-python/2026-07/tracking/response.py
[list-model]: https://raw.githubusercontent.com/AfterShip/tracking-sdk-python/2026-07/tracking/models/get_trackings_response_data.py

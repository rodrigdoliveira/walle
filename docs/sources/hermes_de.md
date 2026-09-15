# Hermes Germany source contract

Observed: 2026-09-14 · Adapter: `hermes-de/2026-09-14.2`

This contract describes the anonymous JSON flow used by Hermes Germany's current public tracking widget. It is a scraper dependency, not a documented compatibility guarantee from Hermes.

## Request flow

1. The public page loads `https://gcp-prd.my-deliveries.de/tnt/bundle/tnt-bundle-v2.js` into `#tnt-app-v2`.
2. Its client uses `https://api.my-deliveries.de`. Walle requests `GET /tnt/v2/shipments/search/{tracking-number}` with `Accept: application/json` and `X-Language: en`.
3. A successful response is an array. Every selected result must contain a `barcode` exactly matching the request.
4. The current widget reads timeline entries from `parcelProgress`, including `parcelStatus`, `status`, `timestamp`, and `historyText`. It also references `atg.companyName`, `parcelAttributes`, and `viewParameters.internationalTrackingLink`.
5. Optional recipient details use `GET /tnt/v2/address/{barcode}` with `X-ZipCode`. This is not called by the initial scraper because it can expose recipient-specific detail and is unnecessary for the basic status timeline.

## Observed validation

A deliberate invalid lookup, `00000000`, returned HTTP 400. The current page's own form accepts 8–20 ASCII letters or digits.

Hermes's official business training document publishes fictional shipment `81529374524354` and linked relabel barcode `97478999781222` in a delivered-shipment example. Both valid-format identifiers now produce `NotFound` through the public endpoint, so they are safe for not-found transport checks but cannot validate a successful history. See the [Hermes business training document](https://www.myhermes.de/content/geschaeftskunden/pdf/myhermesbusiness_trainingdocument_en.pdf).

A private, user-owned international shipment successfully parsed in the `ANNOUNCED` state with one `parcelProgress` event, an RFC 3339 UTC timestamp, and a Correos handoff URL under `viewParameters.internationalTrackingLink`. This confirms the successful-response array, barcode identity, announced event, history text, timestamp, and international-link extraction. The response had no sender name or delivery estimate. The probe redacts both the Hermes number and tracking identifiers embedded in partner URLs.

Later in-transit, pickup, exception, and delivered Hermes responses still need user-owned validation before their status transitions drive notifications.

## Failure rules

- HTTP 400 is invalid input; 404 or a successful empty array is not found; 429 is rate limited; 401/403 is access denied.
- HTML, a non-array JSON root, a missing `parcelProgress`, or a mismatched barcode is not a successful lookup.
- Unknown `parcelStatus` values retain their carrier wording. Terminal states are mapped only from explicit codes or strongly verified event text.
- Parser errors retain the last cached snapshot when integrated into the app.

Sources: [public tracking page](https://www.myhermes.de/empfangen/sendungsverfolgung/), [current tracking bundle](https://gcp-prd.my-deliveries.de/tnt/bundle/tnt-bundle-v2.js).

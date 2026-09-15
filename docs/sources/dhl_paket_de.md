# DHL Paket Germany source contract

Observed: 2026-09-14 · Adapter: `dhl-paket-de/2026-09-14.3`

This contract describes the anonymous JSON flow used by DHL's current public tracking widget. It is a scraper dependency, not a documented compatibility guarantee from DHL.

## Request flow

1. The public page mounts `https://www.dhl.de/int-verfolgen/static/spa/verfolgen.js` with `data-nolp-data-path="/int-verfolgen/data"`.
2. The loader requests `GET /int-verfolgen/data/config?domain=de&language=en` and receives `verfolgenCsrfToken` plus current bundle metadata.
3. Walle issues `GET /int-verfolgen/data/search` with `piececode`, `noRedirect=true`, and `language=en`. It sends `Accept: application/json`, `Content-Type: application/json`, `Verfolgen-CSRF-token`, and `Verfolgen-wg: 0` in the same cookie session.
4. The response root contains `sendungen`. Each result identifies the request through `id`, `sendungsinfo.gesuchteSendungsnummer`, or `sendungsdetails.sendungsnummern.sendungsnummer`.
5. Extra-input flows use `POST /shipment` with JSON such as `piececode`, `zip`, and `international`, or `postedDate`. The adapter implements both request shapes and tests them synthetically; their successful live response still needs verification with a user-owned parcel.

The Rust implementation performs steps 2–5. It never uses the old third-script selector or `initialState` decoder.

## Observed validation

A deliberate invalid lookup returned HTTP 200 with the requested identity, an empty `sendungsverlauf.events`, and `sendungNichtGefunden.keineDatenVerfuegbar=true` plus `keineDhlPaketSendung=true`. This validates the bootstrap, headers, search route, identity binding, and not-found classification.

A private, user-owned DHL Paket international shipment returned eight chronological events from electronic announcement through delivery. This confirmed the found-response identity, `sendungsdetails.sendungsverlauf` fields, timestamps with UTC offsets, locations, international event text, and the delivered root state. One carrier description included an HTML anchor containing the tracking link; adapter version `.2` converts carrier fragments to plain text, and the private probe recursively redacts the number from nested output. The response had no delivery estimate or sender name.

The postcode and shipment-date request bodies have deterministic transport tests, but successful live responses for those optional-input paths still need user-owned examples.

## Failure rules

- A 200 HTML page, absent `sendungen`, empty unclassified array, missing identity, or mismatched identity is not a successful lookup.
- `isRateLimited`, `rateLimited`, or `blockTime` produces a rate-limit error.
- A recognized `sendungNichtGefunden` shape produces `NotFound`.
- `plzBenoetigt` or `versandDatumBenoetigt` produces `NeedsInput` before not-found classification.
- Parser errors retain the last cached snapshot when integrated into the app.

Sources: [public tracking page](https://www.dhl.de/de/privatkunden/dhl-sendungsverfolgung.html), [loader](https://www.dhl.de/int-verfolgen/static/spa/verfolgen.js).

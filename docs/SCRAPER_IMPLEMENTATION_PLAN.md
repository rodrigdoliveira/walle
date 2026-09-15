# DHL and Hermes scraper implementation plan

Version: 1.1 · Date: 2026-09-11 · Status: implementation in progress

This plan expands [DESIGN_SPEC.md](../DESIGN_SPEC.md). Build two fresh Rust adapters, first for DHL Paket Germany and then Hermes Germany, including cross-border routes that their current tracking pages actually expose. Use Docker for checks and build work. The installed Windows Tauri v2 app runs without Docker or a paid tracking API.

The initial Rust scraper library and interactive probe now implement the current anonymous JSON request flows for both carriers. Deliberately invalid requests verified endpoint/error behavior without using real parcel data. Found-parcel parsing and Docker execution still require the validation milestones below before Tauri integration.

## 1. Implementation decisions

| Concern | Planned choice |
| --- | --- |
| Default retrieval | Reused asynchronous `reqwest::Client`; retrieve current HTML or the structured response used by the public tracking page, where the observed flow supports it |
| Parsing | `serde`/`serde_json` for structured data; `scraper` for HTML; pure Rust parsing functions separated from network access |
| JavaScript fallback | Feature-gated Rust `chromiumoxide` worker; experimentally control an isolated installed Edge process on Windows and Chromium in Docker tests |
| Shipment model | One shared snapshot/event model for both carriers; preserve carrier wording and uncertain timestamps |
| Integration order | Inspect both flows first; complete DHL Paket end to end, then Hermes Germany against the same contract |
| Storage and scheduling | Existing design's Rust-owned SQLite repository and scheduler; source adapters do not write the database or send notifications |
| Dependencies | Pin compatible versions, Cargo features, Rust toolchain, and lockfile during scaffolding; no Python, Ruby, PHP, or Node scraper service |
| Scope boundary | DHL Express and other regional services require separate validation; old UK Hermes/Evri is excluded |

`reqwest` supports async requests, configurable redirects, cookies, and typed JSON; `scraper` parses HTML using CSS selectors. These capabilities support the proposed implementation, but do not establish current carrier access. [reqwest documentation](https://docs.rs/reqwest/latest/reqwest/), [scraper documentation](https://docs.rs/scraper/latest/scraper/)

The older repositories remain architectural references only. Their endpoint paths, script positions, HTML selectors, and status phrases are not the new contract. See the [repository review](research/scraper_review.md).

## 2. Inspect and record each current tracking flow

Starting pages:

- [DHL Paket Germany](https://www.dhl.de/de/privatkunden/dhl-sendungsverfolgung.html)
- [Hermes Germany](https://www.myhermes.de/empfangen/sendungsverfolgung/)

Their tracking forms loaded during the previous review; no tracking number was submitted. Data endpoints, required request fields, session behavior, and event schemas remain unverified. Do not invent an endpoint or infer a working scraper from a loaded form.

For each carrier:

1. Use a user-owned parcel in the normal tracking flow. Inspect the rendered result and the requests/responses used to produce it using browser developer tooling. Record redirects and the difference between the initial application shell and shipment data.
2. Identify the lookup method, actual URL, required fields, session bootstrap, content type, localization, and optional postcode step. Separate essential requests from unrelated assets and analytics. Record documented access constraints affecting the chosen source.
3. Compare three extraction options: structured response used by the page, embedded structured data, and rendered event markup. Prefer the least fragile option that works through the observed public flow. An undocumented page endpoint remains a scraper dependency subject to change.
4. Try the same lookup from a clean, app-owned Rust HTTP session. Verify it without copied personal browser cookies, fixed session tokens, or hard-coded expiring headers. Record whether a fresh session and a later repeated lookup both work.
5. If JavaScript is necessary, run the isolated-renderer experiment in section 5. Select a transport per adapter; do not automatically cycle through multiple transports every time a request fails.
6. Compare extracted status, latest event, history, and any estimate against the visible carrier page. Record absent fields as absent. Test cross-border handoffs and postcode behavior with available user-owned examples.

The initial contracts are [DHL Paket Germany](sources/dhl_paket_de.md) and [Hermes Germany](sources/hermes_de.md). They record the observed request flow, response identity binding, extraction anchors, status/time mappings, field availability, transport choice, verification date, and remaining live-validation gaps.

Live input needed during implementation: ideally one active DHL Paket parcel and one active Hermes Germany parcel, with destination country and postcode only when the page requires it. Add completed and cross-border examples as available. Synthetic fixtures cover unavailable states, but must not be described as live coverage. Never reuse strangers' example numbers from old repositories or enumerate numbers.

## 3. Shared Rust structure and contracts

Proposed layout, created during implementation:

```text
crates/
  tracking-core/src/
    model.rs                 # Requests, snapshots, events, outcomes
    source.rs                # Adapter interface and capabilities
    transport/http.rs        # Bounded HTTP, session and redirect handling
    transport/browser.rs     # Optional Rust CDP worker
    carriers/dhl_paket_de.rs  # Source flow and status normalization
    carriers/hermes_de.rs
    parsing/                 # Pure HTML/JSON-to-snapshot functions
  tracking-probe/src/         # Developer lookup/fixture runner
  tracking-mock/src/          # Local fixture server for UI/browser checks
tests/fixtures/
  dhl_paket_de/
  hermes_de/
  browser/                   # Local JS application fixture
docs/sources/                # Observed contracts and coverage
```

Start with small modules. Split a carrier module into fetch/parser/normalizer files only when it becomes useful. Keep the probe and mock server development tools; the installed app calls `tracking-core` directly.

| Contract | Required content |
| --- | --- |
| `TrackingRequest` | Source/service key, original and canonical number, optional destination country/postcode, parcel generation, cancellation context |
| `SourceDocument` | Bounded body or extracted structured data, source/transport/parser version, retrieval time, response classification, evidence tying the result to the request |
| `LookupOutcome` | `Found(snapshot)`, recognized `NotYetFound`, or `NeedsInput(fields)`; errors remain separate |
| `TrackingSnapshot` | Requested identity, normalized state, original carrier status/code, optional ETA/pickup/partner information, events, history completeness, fetched time |
| `TrackingEvent` | Optional source event ID, stable local fingerprint, raw and normalized status, description, optional location, timestamp with precision and timezone certainty |
| `SourceCapabilities` | Required inputs, supported locales, transport, terminal recheck support, tested routes, parser version |

The async adapter performs the verified lookup and invokes a pure parser. The parser takes bytes or a bounded extracted document and returns typed data; it cannot access the network, clock, SQLite, or notifications. Inject transport and clock into orchestration for deterministic tests.

Bind every response to its request. Compare returned number/service when present. For masked or absent identifiers, require the recorded contract to establish binding through the exact request and isolated page context. A mismatched response is an error; a generic application shell is never a shipment result.

The probe accepts private parcel input from an interactive prompt or ignored local input file. Avoid command-line numbers that persist in shell history. Redact identifiers in default output, and never include raw HTML or cookies in ordinary logs.

## 4. Build the HTTP path and carrier parsers

Implement one shared transport with explicit cookie support only where needed, valid HTTPS certificate checks, verified host/redirect allowlists, and query/form encoding. Use current observed inputs rather than string-concatenating a tracking URL. Suggested starting limits: 10-second connect timeout, 30-second total request timeout, three redirects, and 5 MiB of decompressed response data; tune from measurements and document changes.

Classify the response before parsing shipment fields. HTTP 200 can contain a challenge, login page, application shell, or unrelated error. HTTP 404 alone is not a valid parcel-not-found result. Only a recognized carrier response can produce `NotYetFound` or `NeedsInput`.

### DHL Paket

1. Implement the current lookup and optional postcode flow recorded in its source contract.
2. Parse the correct shipment object when a response contains multiple entries. Prefer a recognized structured object or stable semantic container; never select the third script merely because the old library did.
3. Normalize observed status codes first. When only text exists, use tested locale-specific rules anchored to status fields, including negations; a general substring such as “delivered” is insufficient.
4. Preserve the carrier's current summary, complete available timeline, optional ETA, and any explicitly linked delivery partner.
5. Compare the probe result with the carrier page, then repeat from a fresh session.

### Hermes Germany

1. Implement the current German page flow independently. Do not inherit the old UK JSP endpoint or table selectors.
2. Establish whether parcel-shop pickup and delivery-attempt details are available in the same response or a separate detail step.
3. Normalize explicit pickup, delivery, exception, and return information. Missing pickup details stay missing; never manufacture a location or collection deadline.
4. Test any cross-border partner relationship the page actually exposes, keeping linked numbers on the same parcel only when that relationship is explicit.
5. Apply the same probe-versus-page and fresh-session checks as DHL.

For both parsers, tolerate new optional fields but validate required structure and identity. Missing event containers on an unrecognized page mean `source_changed`, not an empty successful history. A recognized status with no timeline can be a partial snapshot. Unknown status values retain their original wording and map to `unknown`.

Store supplied UTC offsets faithfully. Preserve date-only estimates and offset-free event times; never assume every European event uses Berlin time. Do not infer transit stages, delivery dates, or a successful delivery from missing information.

## 5. Conditional browser-rendering experiment

If the HTTP path cannot retrieve a valid result because normal rendering requires JavaScript, prototype `chromiumoxide` with an isolated headless Edge process on Windows. It provides Rust CDP control; Microsoft's Edge documentation describes CDP and separate browser profiles. Edge compatibility with the selected crate and installed browser version is a **test requirement**, not an established result. [chromiumoxide project](https://github.com/mattsse/chromiumoxide), [Edge DevTools Protocol](https://learn.microsoft.com/en-us/microsoft-edge/devtools/protocol/)

Use an explicitly located compatible Edge executable with a new app-owned temporary profile. Start it on demand with hidden windows and a locally restricted debugging connection. Never attach to the user's existing browser or profile. Keep the browser sandbox enabled. Initially detect an installed browser; omit automatic browser downloads. A missing or incompatible executable produces a clear setup error.

Run one browser worker with a bounded queue and isolated page context per lookup. Poll the CDP event handler, navigate the normal page flow, and wait for a source-specific result, not a fixed sleep or generic network-idle signal. Set startup/navigation/job deadlines; extract only the required bounded content. Close each page after its job and release the browser after a short idle period. On Quit, cancellation, or worker failure, close, wait with a deadline, then terminate the app-owned process if necessary. Confirm exit before deleting its temporary profile. [Browser lifecycle API](https://docs.rs/chromiumoxide/latest/chromiumoxide/browser/struct.Browser.html)

Before adopting this transport, pass a local JavaScript fixture in Linux Docker with Chromium and in the installed Windows app with Edge. Verify startup, Unicode/space-containing paths, German text, cancellation, crashes, repeated jobs, no visible windows, no leftover processes, and operation with Docker stopped. Record startup time, peak memory, and idle memory so the browser cost is visible.

Browser rendering is selected to execute a supported page flow. CAPTCHA or access denial pauses checking and offers the carrier website; there is no challenge-solving service, proxy rotation, or automatic transport escalation. No paid API fallback is introduced.

Keep a WebView2-based worker as a later alternative only if the separate browser prerequisite is unacceptable. Its Windows UI-thread/message-pump requirements make it a different integration effort; using WebView2 for Tauri's main UI does not automatically provide a background scraper. [WebView2 threading model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/threading-model)

## 6. Connect refresh, storage, and notifications

Use the intervals in the design spec: initially hourly, 30 minutes in transit, 15 minutes out for delivery, and no automatic polling for terminal or archived parcels. These are app targets; validation may require slower checks.

Keep one active lookup per host, at most two across different hosts, a minimum 30-second gap between lookup starts on one host, and a five-minute manual-refresh cooldown per parcel. Apply these to retries and both transports. A browser's essential page resources belong to its bounded lookup session; they do not each wait 30 seconds. Record request counts, cap sessions/resource consumption, and do not use subrequests to perform extra parcel lookups. Persist host cooldowns across restart.

| Outcome | Scheduler and data behavior |
| --- | --- |
| Valid snapshot | Merge events and save status atomically; update last-success time; then evaluate notification transitions |
| Recognized no updates yet | Retain history, show not found/awaiting events, schedule a later check |
| Needs postcode or service selection | Pause that parcel until input changes |
| Timeout or temporary network/server error | Preserve snapshot; bounded retry through scheduler with exponential backoff and jitter |
| Rate limit | Honor `Retry-After` and a host-wide cooldown; no immediate manual bypass |
| Challenge or explicit access denial | Pause the affected scope and show recovery; do not retry unchanged requests repeatedly |
| Changed schema or mismatched identity | Reject the snapshot, preserve cached data, record parser/source problem; resume only after a controlled successful check |

Permit at most two automatic transient retries per refresh cycle, all through the same scheduler and cooldowns. An unresolved failure returns to a slower due time. A malformed response for one parcel should not immediately disable unrelated services; distinguish parcel-specific input failures from shared source failures.

Merge events using source IDs when reliable, otherwise a versioned fingerprint of stable event fields. Exclude retrieval time from the fingerprint. Preserve old history when a current response is partial. Handle corrected events and backward status transitions explicitly; do not choose the highest enum value as current state. A parser change must not replay historical notifications.

Before committing, verify the parcel still exists, is eligible, and has the same tracking identity generation. A stale response cannot resurrect a deleted parcel or overwrite an edited identity. Establish a notification baseline on first success; notify only relevant new transitions after the database commit. Fetch errors never become delivery notifications.

## 7. Fixtures, Docker, and Windows verification

Create minimal synthetic fixtures based on observed structure, with invented identities and addresses. Keep fixture provenance and expected outcomes next to each case. Private captures, if needed while developing, stay outside committed fixtures and Docker build contexts and are removed after producing sanitized cases.

| Test group | Behaviors to verify |
| --- | --- |
| Carrier parsing | Announced, transit, out for delivery, pickup, delivery attempted, delivered, return, unknown state; optional ETA/time/location; partial history |
| Structure changes | Reordered scripts, extra fields, missing required container, empty shell, malformed JSON, generic 404, 200 challenge, wrong identity |
| Time and identity | Leading zeros, locale variants, offset-free times, date-only ETA, ambiguous DST times, reused numbers, explicit partner links |
| HTTP integration | Request method/encoding, postcode steps, session initialization, bounded redirects/bodies, timeouts, 429 and retry headers |
| Orchestration | Fake-clock due times, host throttles, retries, restart recovery, cancellation, archive/edit/delete races, partial merge, notification deduplication |
| Optional renderer | Local JS result fixture, readiness/error paths, process cleanup, missing browser, Windows/Docker parity limits |

Use Rust `wiremock` as a development dependency for HTTP request/response tests. Implement the separate `tracking-mock` service as a small Rust fixture server for UI and renderer checks; the library itself is not a standalone mock-server executable. [wiremock documentation](https://docs.rs/wiremock/latest/wiremock/)

Planned commands, available after scaffolding:

```sh
docker compose run --rm core-check
docker compose up tracking-mock
docker compose run --rm browser-check   # Only if rendering is selected
docker compose run --rm windows-build
```

`core-check` runs formatting, lint, parser, HTTP, and fake-clock tests from the lockfile. `browser-check` uses a pinned Chromium build against local fixtures. Routine builds never contact live carrier endpoints. Keep mock-host overrides in test/dev composition, not exposed as a generic production URL input.

Prove the minimal Windows dependency set early with the Docker NSIS cross-build, then repeat installation and native tests on Windows. Linux browser success does not prove installed Edge compatibility. Tauri documents NSIS cross-compilation as less tested and MSI as requiring Windows; retain the main spec's documented native-packaging fallback if a concrete blocker remains. [Tauri Windows installers](https://v2.tauri.app/distribute/windows-installer/)

## 8. Work sequence and completion gates

| Milestone | Concrete work | Finished when |
| --- | --- | --- |
| A. Source discovery | Inspect both carrier flows; create source contracts and initial private comparisons; select HTTP or renderer per source | A current user-owned parcel can be retrieved reproducibly for each source, or an exact blocker is documented |
| B. Shared foundation | Scaffold core, probe, synthetic fixture harness, HTTP transport, DTOs, and Docker checks; prototype renderer only if needed | Local fixtures pass; minimal dependency set builds and runs on Windows |
| C. DHL Paket | Implement recorded flow, parser, mapping, postcode handling, and live comparison | Correct real DHL snapshot plus parser/error regressions |
| D. Hermes Germany | Implement independent current flow with pickup/return cases and partner handling | Correct real Hermes snapshot plus parser/error regressions |
| E. App integration | Scheduler, SQLite merge, source health, generation checks, tray and notification path | Deterministic refresh/restart/error cases pass in the app |
| F. Personal release | Installed Windows smoke test, live scheduled comparisons, route matrix, maintenance notes | Both carriers pass the required live checks with Docker stopped |

Discovery and the minimal probe may overlap: only build enough tooling to answer the transport question before expanding the parsers or UI. DHL and Hermes parser work can run independently once the shared contract is fixed.

For live completion, check each carrier at initial lookup, a later eligible scheduled refresh, and after app restart. Compare against the public page. Verify at least one real status transition per carrier as available; until observed, label transition behavior as fixture-tested. Record domestic/cross-border routes and unsupported fields explicitly. Opening the carrier page manually is recovery, not successful automatic tracking.

For maintenance, record parser version and last successful check per source. When a page changes, pause the affected parser, obtain a current user-owned example, add a failing synthetic fixture, update extraction, and rerun the carrier regression suite plus one live comparison. Ship parser changes through an app update; do not introduce remotely downloaded executable parsing rules.

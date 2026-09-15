# Scraper reference review

Reviewed: 2026-09-11. Scope: source age, implementation assumptions, and relevance to Walle's Rust tracking adapters. This is a design review, not a live shipment compatibility test.

## Recommendation

Use `sauladam/shipment-tracker` as a historical reference for separate carrier adapters and normalized events. Develop fresh Rust fetchers and parsers from current carrier responses. Do not translate its old DHL selectors unchanged. Do not use `TheBookPeople/hermes-scraper` as the Hermes Germany implementation: it targets the former UK service, outside Walle's scope.

Scraping is the selected approach following the user's rejection of paid tracking APIs. The first integrations to prove are DHL Paket and Hermes Germany, including representative cross-border deliveries outside the UK. A carrier name alone does not establish all-European route coverage. No paid API subscription is part of the current design.

## Age and scope

| Project | Latest source update | Latest release/tag | Fit for Walle |
| --- | --- | --- | --- |
| `sauladam/shipment-tracker` | 15 June 2022: approximately 4 years, 3 months old | Release 0.7.0: 1 December 2020 | PHP; includes DHL Paket and DHL Express, no Hermes implementation |
| `TheBookPeople/hermes-scraper` | 20 May 2016: approximately 10 years, 4 months old | Tag and RubyGems 0.0.6: 20 May 2016; no GitHub Releases | Ruby; former UK Hermes integration, unsuitable as a Hermes Germany adapter |

Dates refer to default-branch commits, not repository activity timestamps. Neither repository is formally archived. Shipment-tracker's March 2024 `pushed_at` metadata does not mean its default-branch code was updated then. Sources: [shipment-tracker HEAD](https://github.com/sauladam/shipment-tracker/commit/ad1b2e53043f7df7ba05f9bcfe666152f6f8673a), [0.7.0 release](https://github.com/sauladam/shipment-tracker/releases/tag/0.7.0), [supported carriers](https://github.com/sauladam/shipment-tracker#supported-carriers), [Hermes HEAD](https://github.com/TheBookPeople/hermes-scraper/commit/b2dc9465a7a5e5e2264839c1a7179bb9a7f7c099), [RubyGems 0.0.6](https://rubygems.org/gems/hermes-scraper/versions/0.0.6).

## Shipment-tracker: useful ideas, old fetching assumptions

The DHL Paket adapter requests `https://www.dhl.de/int-verfolgen/search`. Its parser selects the third script element, extracts an `initialState: JSON.parse(...)` expression, and expects the nested German shipment-history structure. A positional script selector can break when unrelated page scripts change. Its public tracking link separately points at the old `nolp.dhl.de` route. [DHL implementation](https://github.com/sauladam/shipment-tracker/blob/ad1b2e53043f7df7ba05f9bcfe666152f6f8673a/src/Trackers/DHL.php)

The last substantive Paket request/parser change was 1 December 2020; later DHL edits through June 2022 principally adjust status phrases. The current HEAD date therefore understates the age of its page-extraction strategy. [2020 parser hotfix](https://github.com/sauladam/shipment-tracker/commit/f1edc2e09e8ef61aa0ad6bf8d55247acfd357bc9), [2022 status update](https://github.com/sauladam/shipment-tracker/commit/ad1b2e53043f7df7ba05f9bcfe666152f6f8673a)

The Express adapter last changed on 7 June 2017. It uses the old HTTP `www.dhl.com/shipmentTracking` endpoint and expects a `results[0].checkpoints` response. Treat Express as a separate implementation and validation problem. [Express implementation](https://github.com/sauladam/shipment-tracker/blob/ad1b2e53043f7df7ba05f9bcfe666152f6f8673a/src/Trackers/DHLExpress.php), [last Express change](https://github.com/sauladam/shipment-tracker/commit/21dc5c42ac2b20f9d259deb42089fa6e43ea8e0d)

Historical failure reports help identify test cases, but are not present-day compatibility results:

- Issue #28 reports blocked automated requests. The author later clarified that the DHL report concerned **Express**, with regular DHL untested. Do not cite it as proof that DHL Paket currently fails. [Issue](https://github.com/sauladam/shipment-tracker/issues/28), [clarification](https://github.com/sauladam/shipment-tracker/issues/28#issuecomment-999999509)
- Issue #26 reports a postcode-required shipment without a known working parameter. Preserve a `needs_input` state and verify the current postcode flow. [Postcode issue](https://github.com/sauladam/shipment-tracker/issues/26)

The package manifest declares MIT; the reviewed tree has no separate license file. [Manifest](https://github.com/sauladam/shipment-tracker/blob/ad1b2e53043f7df7ba05f9bcfe666152f6f8673a/composer.json)

## Hermes-scraper: wrong region and brittle parser

Its hard-coded URL is on `hermes-europe.co.uk`, ending in `customerparceltrackingservice/trackingDetailsHermes.jsp`. It extracts table rows with the exact `trackingText` class and expects three text values per row. If a redesigned page contains no matching rows, the method returns an empty list rather than identifying a format change. [Implementation](https://github.com/TheBookPeople/hermes-scraper/blob/master/lib/hermes/scraper.rb)

The gem's Mechanize constraint permits versions >=2.7.4 and <3.0, and its CI configuration targets Ruby 2.3.0 with Bundler 1.12.1. Its shipment test uses a recorded 2016 response; passing that fixture would not demonstrate compatibility with today's tracking website. The project declares GPL-3.0. [Gemspec](https://github.com/TheBookPeople/hermes-scraper/blob/master/hermes-scraper.gemspec), [CI configuration](https://github.com/TheBookPeople/hermes-scraper/blob/master/.travis.yml), [tests](https://github.com/TheBookPeople/hermes-scraper/blob/master/spec/hermes/scraper_spec.rb)

UK Hermes became Evri in March 2022. This repository is not evidence of support for Hermes Germany or the user's non-UK deliveries. [Evri history](https://www.evri.com/about-us)

## What was checked on current carrier sites

Both current public tracking pages loaded their tracking-number forms in a JavaScript-capable browser. DHL's rendered page exposed the `nolp` container; Hermes Germany exposed `tnt-app-v2`. These observations show that modern application forms are present; they do not identify a stable shipment-data contract. Static page extraction alone did not provide tracking events. [DHL tracking](https://www.dhl.de/de/privatkunden/dhl-sendungsverfolgung.html), [Hermes Germany tracking](https://www.myhermes.de/empfangen/sendungsverfolgung/)

No tracking number was submitted, no old sample number was reused, and neither repository's code or dependencies were executed. Current event parsing, postcode submission, automated access, and cross-border completeness remain unverified. Age and implementation fragility justify replacing the old assumptions, but do not prove every legacy endpoint is dead.

## Consequences for the Rust design

1. Prove lookup on user-owned DHL Paket and Hermes Germany parcels before committing to a transport. Determine whether current HTML or page-fetched structured data can be retrieved with Rust HTTP requests, or whether an isolated JavaScript-capable renderer is necessary.
2. Keep transport and parsing separate. Use typed snapshots, source/parser versions, and synthetic fixtures. Parse observed status codes or locale-specific wording conservatively, preserving unknown values and time precision.
3. Distinguish a valid empty tracking result from an unexpected page, challenge, or parser failure. Failed checks preserve cached shipment state and cannot emit delivery notifications.
4. Use conservative per-host scheduling, persisted cooldowns, and bounded retries. A blocked or changed source pauses automatic checks and offers the carrier page as recovery; browser fallback alone does not meet the automatic-tracking requirement.
5. Keep the application core in Rust and test it in Docker. If rendering is required, prove that component's Windows packaging, isolation, and Docker build compatibility early. The installed Tauri app runs without Docker.

The earlier [AfterShip guide](../references/aftership_hermes_dhl_guide.md) remains preserved as historical reference. It is not the active implementation contract. See the current [design specification](../../DESIGN_SPEC.md).

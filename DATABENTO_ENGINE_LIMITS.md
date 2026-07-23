# Databento Engine Limits

This is the current binding boundary for strategy designs using the engine's Databento historical
provider. It lists supported behavior once and keeps only unresolved limitations.

## Supported historical paths

| Product | Engine behavior |
|---|---|
| OHLCV `1s`, `1m`, `1h`, `1d` | Raw immutable CSV cache with a versioned checksum manifest |
| Equity adjustment factors | Separately materialized split-adjusted or total-return-adjusted CSV with factor provenance |
| Trades | Provenance-rich normalized JSONL replay; deterministic UTC trade-to-bar materialization |
| BBO / MBP-1 | Bid/ask replay and quote-driven fills |
| MBP-10 | L2 snapshots and displayed-depth-bounded per-order VWAP fills |
| Status and imbalance | First-class timestamped venue events |
| Statistics | Official settlement, open-interest, volume, and price statistics kept separate from bars |
| Definitions | Point-in-time typed metadata import for supported equities and dated futures |

Every paid request is preceded by capability/date/schema validation and an exact cost estimate.
Raw caches are written atomically and reused only when the request manifest and file checksum match.

## Current hard limits

- One Databento cache request represents one requested symbol; provider-side atomic baskets are not
  implemented.
- Continuous and parent symbology are rejected until a real futures roll policy and ledger exist.
- Adjustment policies apply only to OHLCV materializations and never overwrite the raw cache.
- Databento option definitions are not applied to replay because exercise style is absent from the
  normalized definition fields used by the engine. Local options are explicit European
  cash-settled contracts only.
- MBP-10 is a bounded snapshot model, not persistent depletion, queue position, or market impact.
- MBO/L3 is not accepted.
- Live streaming is outside this historical provider path.
- Spot FX, perpetuals, crypto spot, bonds, and OTC products require separate verified source and
  economics contracts.

## Design rule

If a strategy needs an unsupported product or market model, it must either:

1. use an explicitly named proxy whose report does not claim paper replication, or
2. fail closed and name the exact missing schema, reference data, or economic convention.

Do not substitute futures for spot, bar volume for queue state, a UTC close for official
settlement, or European cash settlement for an unknown option contract.

Primary provider references:

- [Dataset capability metadata](https://databento.com/docs/api-reference-historical/basics/datasets)
- [Historical range requests and cost](https://databento.com/docs/api-reference-historical/timeseries/timeseries-get-range?historical=http&live=http)
- [Point-in-time instrument definitions](https://databento.com/docs/schemas-and-data-formats/instrument-definitions)
- [Adjustment factors](https://databento.com/docs/venues-and-datasets/adjustment-factors)

# Remaining Engine Gaps

This file tracks only limitations that still exist in the current engine. Resolved items are
removed rather than retained as a historical checklist. Git history records the completed work.

## Databento acquisition

### DB-2: Atomic multi-symbol requests

The replay engine supports timestamp-merged local multi-instrument sources, but one Databento
provider request still materializes one requested symbol. The cache manifest is versioned and
checksum-bound, but it is not yet an atomic aligned-basket manifest with per-record resolved
symbols and publishers.

**Done when:** one paid request can materialize a symbol-aware basket atomically, partial output
cannot be accepted as complete, and every resolved symbol and publisher is recorded.

### DB-5: Futures rolls

Dated futures and official statistics are supported. Continuous and parent symbology are rejected
because there is no configured roll rule or auditable close/open ledger.

**Done when:** a configured two-contract fixture produces exactly one explicit roll transition,
preserves multiplier PnL, and identifies the active dated contract at each fill.

### DB-6: Listed options beyond European cash settlement

Local replay supports typed call/put metadata, linked underlying data, signed multi-leg positions,
and deterministic European cash settlement. Databento option definitions are rejected because
their normalized definition record does not establish exercise style.

American exercise, physical assignment, corporate-action-adjusted deliverables, OPRA chain
acquisition, Greeks, and implied-volatility analytics remain unsupported.

**Done when:** exercise style and settlement terms come from a verified source, American and
physical-assignment cash flows have deterministic fixtures, and OPRA replay passes the complete
option suite.

### DB-8: MBO/L3 reconstruction

Trades, BBO/MBP-1, and MBP-10 are normalized replay inputs. MBP-10 market fills are bounded by the
displayed snapshot and labeled as L2 simulation. Snapshot liquidity is not persistently depleted
between orders.

MBO/L3 remains unsupported because reset/reconnect handling, sequence-complete reconstruction,
and an explicit queue-position model do not exist.

**Done when:** reconstruction survives checked snapshot/reset/reconnect fixtures and the selected
queue model is named in every L3-simulated fill.

## Research workflow

### Generic feature cache

No generic `(strategy, parameters, bars)` feature cache is provided. External strategies do not
share a stable feature ABI, so strategy names and parameters are insufficient to prove two cached
features are equivalent. `pallas-resample` provides explicit immutable bar materialization, and
`pallas-sweep` provides bounded parallel catalog execution.

Add a generic cache only after feature identity, versioning, dependencies, and invalidation are
part of a stable protocol.

## Products awaiting a concrete source contract

- Databento spot FX remains gated on capability inspection finding a real historical product.
- Perpetuals require historical funding, mark, and index data.
- Crypto spot requires a venue-specific historical adapter.
- Bonds require coupon, accrual, call, and settlement conventions.
- OTC products require product-specific curves and quote contracts.

None of these may be advertised through a generic asset-class label without reproducible source
data, typed economics, cash-flow tests, and source attribution in reports.

## Research-proxy warning

OHLCV proxies for tick, book, options-volatility, or cross-asset studies remain proxies. A passing
backtest does not make them a replication of the source paper. Strategies that require unavailable
MBO, option-surface, or second-leg data must continue to label the approximation or fail closed.

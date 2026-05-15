# Payments Engine

A toy payments engine that processes financial transactions from CSV and outputs client account states.

## Usage

```bash
cargo run -- transactions.csv > accounts.csv
```

## Transaction Types

| Type | Effect |
|------|--------|
| `deposit` | Credits `available` and `total` |
| `withdrawal` | Debits `available` and `total` (fails if insufficient funds) |
| `dispute` | Moves amount from `available` to `held` (by tx reference) |
| `resolve` | Returns amount from `held` to `available` (by tx reference) |
| `chargeback` | Removes amount from `held` and `total`, freezes account |

## Design Decisions

- **rust_decimal** — Avoids floating-point rounding errors; exact 4-decimal-place precision
- **Streaming** — CSV is read row-by-row, never loaded fully into memory
- **HashMap lookups** — O(1) client and transaction access; only successful transactions are stored
- **Locked accounts** — All operations on locked accounts are silently ignored (consistent with "frozen" semantics)
- **No `.clone()` in hot path** — `dispute`/`resolve`/`chargeback` use borrow scopes with `Copy` field extraction instead of cloning `TransactionRecord`

## Dispute on Withdrawals

Disputing a withdrawal returns the withdrawn funds to `available` and places them in `held`. On chargeback, `held` and `total` decrease while `available` stays the same — the withdrawal is effectively reversed.

## Testing

- **Unit tests** (27) in `src/engine.rs` and `src/models.rs` — cover each operation, invariants, boundaries, and guards
- **Integration tests** (12) in `tests/integration.rs` — end-to-end CSV → engine → CSV with parsed output assertions

Run all tests:
```bash
cargo test
```

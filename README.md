# Payments Engine

A streaming transaction processor that reads financial transactions from CSV, maintains client account state, and outputs final balances.

## Usage

```bash
cargo run -- data/sample.csv > accounts.csv
```

Output is a CSV written to stdout with one row per client, sorted by client ID:

```
client,available,held,total,locked
1,1.5000,0.0000,1.5000,false
2,2.0000,0.0000,2.0000,false
```

## Input Format

The input CSV has a header row followed by transaction records.

| Column   | Type          | Description                                  |
|----------|---------------|----------------------------------------------|
| `type`   | `string`      | Transaction type (`deposit`, `withdrawal`, `dispute`, `resolve`, `chargeback`) |
| `client` | `u16`         | Client identifier                            |
| `tx`     | `u32`         | Transaction reference ID                     |
| `amount` | `decimal (4dp)` | Transaction amount. Empty for `dispute`, `resolve`, `chargeback`. |

Amounts are parsed as fixed-point decimals with at most 4 decimal places. Values exceeding 4dp are rejected at deserialization.

## Output Format

| Column      | Type          | Description                              |
|-------------|---------------|------------------------------------------|
| `client`    | `u16`         | Client identifier                        |
| `available` | `decimal (4dp)` | Funds available for withdrawals          |
| `held`      | `decimal (4dp)` | Funds held due to open disputes          |
| `total`     | `decimal (4dp)` | `available + held` (invariant preserved except after withdrawal chargeback) |
| `locked`    | `bool`        | `true` after any chargeback on the account |

## Transaction Types

| Type          | Amount | Effect                                                       | Validation                                        |
|---------------|--------|--------------------------------------------------------------|---------------------------------------------------|
| `deposit`     | Required | `available += amount`, `total += amount`                     | Must be > 0                                       |
| `withdrawal`  | Required | `available -= amount`, `total -= amount`                     | Must be > 0 and <= `available`                    |
| `dispute`     | None    | Moves referenced tx amount from `available` to `held`       | Tx must exist, belong to client, not already disputed |
| `resolve`     | None    | Moves referenced tx amount from `held` back to `available`  | Tx must exist, belong to client, and be under dispute |
| `chargeback`  | None    | Removes amount from `held` and `total`, locks account        | Tx must exist, belong to client, and be under dispute |

Unknown transaction types are silently ignored.

## Dispute Lifecycle

Transactions follow a state machine for dispute handling:

```
         dispute
Normal ────────────> UnderDispute
                          │          │
              ┌───────────┘          └──────────┐
              │ resolve                         │ chargeback
              ▼                                 ▼
         Normal (reverted)                Finalized (locked)
```

- **Normal**: Initial state. Funds are in `available`.
- **UnderDispute**: Funds are in `held`. The transaction is flagged.
- **Resolved**: Dispute dismissed. Funds return to `available`.
- **Finalized**: Dispute upheld. Funds removed from `held` and `total`. Account is permanently locked -- all subsequent operations are silently ignored.

Disputing a withdrawal reverses its effect: `available` is restored and the withdrawn amount moves to `held`. On chargeback, `held` and `total` decrease while `available` stays unchanged.

**Withdrawal chargeback invariant note:** After a withdrawal chargeback, `total != available + held`. Example: deposit 100, withdraw 30, dispute withdrawal, chargeback yields `available=100, held=0, total=70`. The withdrawal is permanently reversed in `available` while `total` tracks net completed transactions.

## Design Decisions

- **`rust_decimal`** -- Fixed-point arithmetic avoids floating-point rounding errors; enforces 4dp precision at the boundary between parsing and processing.
- **Streaming** -- CSV is read row-by-row via `csv::Reader`, never loaded fully into memory.
- **HashMap lookups** -- O(1) client and transaction access. Only successful deposits and withdrawals are stored.
- **Locked accounts** -- All operations on locked accounts are silently ignored. This is checked once at the top of `process_transaction` and again before each mutation to cover internal calls.
- **No `.clone()` in dispute path** -- `lookup_tx` extracts `Copy` fields (`TransactionKind`, `Decimal`) from the borrowed `TransactionRecord` instead of cloning the full struct.
- **Amount validation at parse time** -- Negative and zero amounts, and values with more than 4 decimal places, are rejected during CSV deserialization before they reach the engine.
- **Blank line tolerance** -- Empty rows (all fields blank after trimming) are skipped silently.

## Architecture

The codebase is structured as three modules in a single crate.

**`src/models.rs`** -- Data structures and serialization. Defines `TransactionRow` (CSV input with custom decimal deserializer that enforces 4dp), `TransactionRecord` (internal store for dispute lookups), and `Client` (output state with `apply_dispute`/`apply_resolve` methods that encapsulate fund movement logic per transaction kind).

**`src/engine.rs`** -- Business logic. `PaymentEngine` holds a `HashMap<u16, Client>` and `HashMap<u32, TransactionRecord>`. The `process_transaction` method dispatches to typed handlers. A shared `lookup_tx` helper validates dispute prerequisites (tx exists, belongs to client, correct dispute state). All mutations guard against locked accounts.

**`src/main.rs`** -- CLI entry point. Reads a CSV file path from args, streams rows through the engine, skips blank lines, and writes sorted client output to stdout.

## Testing

53 tests total: 37 unit tests + 16 integration tests.

**Unit tests** (`src/models.rs`, `src/engine.rs`) cover:
- Basic operations (deposit, withdrawal, dispute, resolve, chargeback)
- Dispute on withdrawals and the available/total/held invariant
- Locked account rejection for all transaction types
- Invalid references (nonexistent tx, wrong client, non-disputed tx for resolve/chargeback)
- State transition guards (double dispute, resolve/chargeback after chargeback)
- Amount validation (zero, negative)
- Boundary conditions (exact available withdrawal, auto-client creation, default state)

**Integration tests** (`tests/integration.rs`) cover:
- End-to-end CSV in, CSV out with parsed output assertions
- Whitespace tolerance in CSV fields
- Unknown transaction types and missing amounts
- Unordered transaction IDs
- Duplicate tx ID behavior (record overwrite)
- Blank line handling
- 4dp precision enforcement at parse boundary

## Running

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run the engine
cargo run -- <transactions.csv>

# Lint
cargo clippy
```

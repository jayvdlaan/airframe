# airframe_tabular

Short description: Config-driven tabular ingest (CSV/TSV) → typed rows.

## Overview

`airframe_tabular` turns heterogeneous tabular sources (bank CSVs, expense exports,
log files, …) into uniform `Row` values via a declarative `Profile`. The profile
maps your *logical* field names (`date`, `amount`, `payee`) to the actual column
headers in a given source, plus parser knobs (delimiter, header skip, encoding).

This crate is intentionally narrow: it does not own storage (use `airframe_data`),
encoding/decoding of typed values (use `airframe_codec`), or domain interpretation
(your app does that). It is the bytes → rows step.

## Logical pieces

- `Profile` — TOML-loadable description of one tabular source: delimiter, quoting,
  header presence, skip lines, and a `ColumnMap` (`logical → header`).
- `Row` — a parsed record exposed as `logical_name → string value`.
- `read_rows(bytes, &profile)` — driver function.
- `parse` — small helpers for locale-aware date and decimal parsing.

## Airframe module compatibility

- Compatibility: No — this crate is a library; it does not export an Airframe module.

## Layer

L1 Primitives (alongside `airframe_codec`).

## Example

```rust
use airframe_tabular::{Profile, read_rows, parse};

let bytes = std::fs::read("statement.csv").unwrap();
let profile: Profile = toml::from_str(include_str!("../profiles/ing.toml")).unwrap();
let rows = read_rows(&bytes, &profile).unwrap();

for row in &rows {
    let date = parse::date(row.get("date").unwrap(), "%Y-%m-%d").unwrap();
    let amount = parse::decimal_european(row.get("amount").unwrap()).unwrap();
    println!("{date} {amount}");
}
```

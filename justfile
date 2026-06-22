# Airframe — application framework
# Usage: just <recipe> [args]

set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes
default:
    @just --list

# Build all crates
build:
    cargo build --workspace

# Run all tests
test:
    cargo test --workspace

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Apply formatting
fmt:
    cargo fmt --all

# Generate workspace documentation
doc:
    cargo doc --workspace --no-deps

# Full pre-publish gate — run manually. This project has NO CI by design
# (supply-chain risk reduction); this recipe is the gate. Must be fully green
# before publishing or merging a contribution.
release-check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo build --workspace
    # advertised non-default feature combinations
    cargo build -p airframe_db --features module
    cargo build -p airframe_mysql --features module,driver
    cargo build -p airframe_sqlite --features module
    cargo build -p airframe_winreg --features health,config
    cargo build -p airframe_kv --features kv-fs
    cargo build -p airframe_scheduler --features airframe-spacetime
    cargo build -p airframe_prefab --features http,config
    cargo build -p airframe_sdata --features integration-pdata
    cargo build -p airframe_pdata --features compress
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Publish every crate to crates.io in dependency order.
# IRREVERSIBLE: crates.io versions and names are permanent (yank != delete).
# PREREQUISITE: all spacetime-* crates must already be published to crates.io
# (run `just publish` in the spacetime repo first). Run `just release-check`
# first. cargo blocks until each crate is indexed before publishing the next.
# If interrupted, comment out the crates already published and re-run.
publish:
    cargo publish -p airframe_api
    cargo publish -p airframe_core
    cargo publish -p airframe_macros
    cargo publish -p airframe_args
    cargo publish -p airframe_crypt
    cargo publish -p airframe_audit
    cargo publish -p airframe_channel
    cargo publish -p airframe_config
    cargo publish -p airframe_codec
    cargo publish -p airframe_compress
    cargo publish -p airframe_data
    cargo publish -p airframe_http
    cargo publish -p airframe_health
    cargo publish -p airframe_db
    cargo publish -p airframe_event
    cargo publish -p airframe_id
    cargo publish -p airframe_ipc
    cargo publish -p airframe_kv
    cargo publish -p airframe_log_api
    cargo publish -p airframe_logging
    cargo publish -p airframe_metrics
    cargo publish -p airframe_mysql
    cargo publish -p airframe_net
    cargo publish -p airframe_secrets
    cargo publish -p airframe_pdata
    cargo publish -p airframe_pg
    cargo publish -p airframe_scheduler
    cargo publish -p airframe_prefab
    cargo publish -p airframe_recovery_bundle
    cargo publish -p airframe_redis
    cargo publish -p airframe_sdata
    cargo publish -p airframe_sqlite
    cargo publish -p airframe_tabular
    cargo publish -p airframe_winreg
    cargo publish -p airframe_wire

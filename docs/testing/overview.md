# Testing

## Test framework

MESH uses Rust's built-in test harness across the Cargo workspace. Tests are
primarily colocated in crate `src/` trees, with additional integration and
performance scenarios under crate `tests/` directories.

Use the Nix development environment so native Wayland and font dependencies are
available.

## Running tests

Run the complete workspace:

```bash
nix develop -c cargo test --workspace
```

Cover optional features with this matrix. The CLI's Tracy and allocation
profilers use different global allocators and must be tested separately:

```bash
nix develop -c cargo test --workspace --exclude mesh-tools-cli --all-features
nix develop -c cargo test -p mesh-tools-cli --features allocation-profiling
nix develop -c cargo test -p mesh-tools-cli --features perf-tracy
```

Run one crate:

```bash
nix develop -c cargo test -p mesh-core-module
nix develop -c cargo test -p mesh-core-shell --lib
```

Run one named test:

```bash
nix develop -c cargo test -p mesh-core-shell test_name -- --exact
```

Run ignored release-mode performance tests explicitly:

```bash
nix develop -c cargo test --release --workspace -- --ignored --nocapture
```

## Writing tests

- Place focused unit tests in a local `tests` module or adjacent `tests.rs`.
- Put larger crate integration scenarios in the crate's `tests/` directory.
- Reuse helpers under `crates/core/shell/src/shell/component/tests/` for shell
  component and shipped-surface scenarios.
- For module contracts, test both accepted normalization and diagnostic failure
  paths.
- For renderer or interaction changes, include behavior assertions rather than
  relying only on snapshots or timing.

## Scrolling contract checks

The `Scrolling contract` workflow runs on pull requests, pushes to `main`, and
manual dispatches. It checks core defaults with an empty theme, repeated wheel
and touchpad input across all three scroll tags, horizontal and vertical axes,
three display scales, boundary reversal, padded launcher pixels and geometry,
momentum, popup progress while its parent is blocked, and clipped visual bounds.
The shell checks compile with `gpu` enabled and use headless raster fixtures;
hardware GPU workload coverage remains in the performance workflow.

Run the generic handler-free input matrix locally:

```bash
nix develop -c cargo test -p mesh-core-shell --features gpu --lib scroll_contract
```

## Performance regression checks

The repository stores workload tolerances in
`config/performance-baseline.tsv`. Run:

```bash
nix develop -c ./tools/check-performance
```

The `Performance regression` GitHub Actions workflow runs this command for pull
requests, pushes to `main`, and manual dispatches.

## Coverage

No line, branch, or function coverage threshold is configured in the
repository. Verification is based on test behavior and explicit performance
workloads.

<!-- generated-by: gsd-doc-writer -->
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

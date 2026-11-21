# Repository Guidelines

## Project Structure & Module Organization

RustCraft is a Cargo workspace. Source and tests live under `crates/`, and native assets live under `assets/`. Use the [module boundaries](doc/architecture.md#section-3) and [development phases](doc/development.md#section-16) to place work. Route authoritative state changes through the simulation's transaction API. Keep rendering and protocol adapters outside the authoritative core. Follow the [documentation standard](doc/documentation-standard.md) when updating the [engineering docs](doc/README.md).

## Build, Test, and Development Commands

Run these commands from the repository root.

- `cargo build --locked` compiles the default workspace targets. See the [project README](README.md) for client and server startup commands.
- `cargo test --workspace` runs workspace tests.
- `cargo fmt --all -- --check` checks formatting, and `cargo clippy --workspace --all-targets` checks for common Rust issues.
- `python3 tools/check_docs.py` checks doc paths and local links.

## Coding Style & Naming Conventions

For Rust contributions, use four-space indentation and format with `cargo fmt --all`. Use `snake_case` for modules, functions, and test names, `UpperCamelCase` for types and traits, and `SCREAMING_SNAKE_CASE` for constants. Preserve the specification's EXPLICIT, DEFAULT, and TBD labels when documenting decisions.

## Testing Guidelines

Use Rust's built-in test harness. Name tests for observable behavior, such as `rejects_stale_mesh_result`. The [quality gates](doc/development.md#section-15) cover palettes and registries, chunk lifecycle, save/load, and deterministic behavior with fixed seeds, inputs, and rules. Use the applicable phase gates to determine test scope.

## Commit & Pull Request Guidelines

Follow the [branch naming rules](doc/branch-naming.md) and use short, imperative commit subjects such as “Document chunk lifecycle invariants.” In pull requests, summarize the change, identify relevant specification sections or decision IDs, report validation, and link related issues when available. For changes to confirmed RCF decisions, record the rationale, affected modules, compatibility risks, migration impact, tests, and decision source.

R7：B2 分支独立修改，用于验证未合并分支。

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

poly-hft is a Rust project (high-frequency trading related, based on the name). Currently in early development with minimal scaffolding.

## Build Commands

```bash
cargo build          # Build the project
cargo build --release # Build with optimizations
cargo run            # Run the application
cargo test           # Run all tests
cargo test <name>    # Run tests matching <name>
cargo clippy         # Run linter
cargo fmt            # Format code
```

## Note

The Cargo.toml currently specifies `edition = "2024"` which is invalid. It should be `edition = "2021"` (the latest stable Rust edition).

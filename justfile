set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set dotenv-load

default:
    @just --list

# aliases
alias b := build
alias t := test
alias l := lint
alias f := fmt
alias c := ci

[group('dev')]
fmt:
    cargo fmt --all

[group('dev')]
fmt-check:
    cargo fmt --all -- --check

[group('dev')]
lint:
    cargo clippy --all-targets -- -D warnings

[group('dev')]
test *ARGS:
    cargo test --release {{ ARGS }}

[group('dev')]
build:
    cargo build --release

# fmt-check + lint + test. Mirrors what CI runs.
[group('dev')]
ci: fmt-check lint test

# fmt + lint + test + build
[group('dev')]
all: fmt-check lint test build

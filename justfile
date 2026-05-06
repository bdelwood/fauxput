set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set dotenv-load

# Path to the vkms-edid-dkms source checkout. Override with
# `VKMS_SRC=/path/to/src just vkms-insmod` or in env file.
VKMS_SRC := env_var_or_default("VKMS_SRC", env_var("HOME") / "Documents/git/github/vkms-edid-dkms")

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

# fmt + lint + test + build + docs
[group('dev')]
all: fmt-check lint test build docs

[group('docs')]
docs-rs:
    cargo doc --no-deps --lib

[group('docs')]
docs-book:
    cd docs && mdbook build

# Combined docs site (rustdoc + mdbook)
[group('docs')]
docs: docs-rs docs-book
    rm -rf public
    mkdir -p public/rustdoc
    cp -R docs/book/. public/
    cp -R target/doc/. public/rustdoc/

[group('docs')]
docs-serve:
    cd docs && mdbook serve --open

# use insmod to temporarily load patched vkms
[group('vkms')]
vkms-insmod:
    make -C {{ VKMS_SRC }} -s
    sudo modprobe -r vkms
    sudo insmod {{ VKMS_SRC }}/vkms.ko create_default_dev=0

# register vkms with dkms
[group('vkms')]
vkms-dkms:
    sudo cp -rT {{ VKMS_SRC }} /usr/src/vkms-edid-0.1
    sudo dkms add -m vkms-edid -v 0.1
    sudo dkms install -m vkms-edid -v 0.1
    sudo modprobe -r vkms
    sudo modprobe vkms

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

# Build the dev PKGBUILD.
[group('pkg')]
pkg:
    mkdir -p .dev-pkg
    cp packaging/arch/fauxput.install .dev-pkg/fauxput.install
    sed \
        -e 's|^source=.*|source=()|' \
        -e 's|^sha256sums=.*|sha256sums=()|' \
        -e 's|cd "$srcdir/$pkgname-$pkgver"|cd "{{ justfile_directory() }}"|g' \
        packaging/arch/PKGBUILD > .dev-pkg/PKGBUILD
    cd .dev-pkg && makepkg -f --noconfirm --nocheck

# Build + install dev PKGBUILD.
[group('pkg')]
pkg-install: pkg
    sudo pacman -U --noconfirm $(ls -1t .dev-pkg/fauxput-*.pkg.tar.zst | head -1)
    fauxput reset --yes 2>/dev/null || true
    @echo "==> done. Verify: getcap /usr/bin/fauxput"

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

# helpers for quick testing
[group('runtime')]
up:
    fauxput up --width 1920 --height 1080 --fps 60 --primary

[group('runtime')]
down:
    fauxput down

[group('runtime')]
reset:
    fauxput reset --yes

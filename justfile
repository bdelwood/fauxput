set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set dotenv-load

# dev submod
mod dev 'dev/justfile'

# Path to the vkms-edid-dkms source checkout.
VKMS_SRC := env_var("VKMS_SRC")

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

# Push Cargo.toml's keywords to the GitHub repo's topics list.
[group('dev')]
sync-topics:
    @KEYWORDS=$(cargo metadata --format-version 1 --no-deps | jq -c '.packages[0].keywords') && \
        REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner) && \
        echo "==> setting topics on $REPO: $KEYWORDS" && \
        gh api -X PUT "repos/$REPO/topics" --input - <<<"{\"names\": $KEYWORDS}"

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

# Build the dev PKGBUILD against the working tree.
[group('pkg')]
pkg:
    mkdir -p .dev-pkg
    cp packaging/arch/fauxput.install .dev-pkg/fauxput.install
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/') && \
        sed \
            -e "s|^pkgver=__PLACEHOLDER__|pkgver=$VERSION|" \
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

# Sync the latest tag to BOTH AUR repos (source + bin); requires AUR_REPO and AUR_REPO_BIN env vars.
[group('pkg')]
aur-sync: aur-sync-source aur-sync-bin

# Sync the latest tag to the source AUR repo (requires AUR_REPO env var).
[group('pkg')]
aur-sync-source:
    @just _aur-sync packaging/arch "${AUR_REPO:?set AUR_REPO to your AUR clone path}"

# Sync the latest tag's prebuilt binary to the fauxput-bin AUR repo (requires AUR_REPO_BIN env var).
[group('pkg')]
aur-sync-bin:
    @just _aur-sync packaging/arch-bin "${AUR_REPO_BIN:?set AUR_REPO_BIN to your fauxput-bin AUR clone path}"

# Internal: render {{TEMPLATE_DIR}}/PKGBUILD into {{AUR_REPO}}, build, commit, push.
[private]
_aur-sync TEMPLATE_DIR AUR_REPO:
    VERSION=$(git describe --tags --abbrev=0 | sed 's/^v//') && \
        echo "==> syncing v$VERSION to {{AUR_REPO}}" && \
        sed "s|^pkgver=__PLACEHOLDER__|pkgver=$VERSION|" \
            "{{TEMPLATE_DIR}}/PKGBUILD" > "{{AUR_REPO}}/PKGBUILD" && \
        cp "{{TEMPLATE_DIR}}/fauxput.install" "{{AUR_REPO}}/" && \
        cd "{{AUR_REPO}}" && \
        updpkgsums && \
        makepkg -f --noconfirm && \
        makepkg --printsrcinfo > .SRCINFO && \
        git add PKGBUILD fauxput.install .SRCINFO && \
        git commit -m "Update to v$VERSION" && \
        git push origin master

# use insmod to temporarily load patched vkms
[group('vkms')]
vkms-insmod:
    make -C {{ VKMS_SRC }} -s
    sudo modprobe -r vkms
    sudo insmod {{ VKMS_SRC }}/vkms.ko create_default_dev=0

# Build/rebuild & register vkms-edid-dkms on the host.
[group('vkms')]
vkms-dkms:
    -fauxput reset --yes 2>/dev/null
    -sudo dkms remove -m vkms-edid -v 0.1 --all 2>/dev/null
    sudo rm -rf /usr/src/vkms-edid-0.1
    sudo cp -rT {{ VKMS_SRC }} /usr/src/vkms-edid-0.1
    sudo dkms add -m vkms-edid -v 0.1
    sudo dkms install -m vkms-edid -v 0.1
    -sudo modprobe -r vkms 2>/dev/null
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

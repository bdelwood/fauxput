set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set dotenv-load

# dev submod
mod dev 'dev/justfile'

# Path to the vkms-edid-dkms source checkout.
VKMS_SRC := env_var_or_default("VKMS_SRC", "")

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

# Prepare v5 VKMS sources with the compatibility patches for Linux 7.1 or 7.2.
[group('vkms')]
[positional-arguments]
vkms-prepare kernel_series source output:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ ! $1 =~ ^7[.][12]$ ]]; then
        echo 'Kernel series must be 7.1 or 7.2' >&2
        exit 2
    fi
    kernel_series=$1
    source_dir=$(realpath -e -- "$2/drivers/gpu/drm/vkms")
    output_dir=$(realpath -m -- "$3")
    snippets_dir="$PWD/docs/src/snippets"
    patchset_dir="$snippets_dir/vkms"
    if [[ -e $output_dir || -L $output_dir ]]; then
        echo "Output already exists: $output_dir" >&2
        exit 1
    fi
    # Accept only the pinned upstream files, before applying local patches.
    (cd "$source_dir" && sha256sum --check "$patchset_dir/upstream.sha256")
    mkdir -- "$output_dir"
    while read -r checksum source_file; do
        install -m 0644 -- "$source_dir/$source_file" "$output_dir/$source_file"
    done < "$patchset_dir/upstream.sha256"
    install -m 0644 -- "$snippets_dir/Makefile" "$output_dir/Makefile"
    install -m 0644 -- "$snippets_dir/vkms_oot_shim.h" "$output_dir/vkms_oot_shim.h"
    sed -e "s/^PACKAGE_VERSION=.*/PACKAGE_VERSION=\"0.3-$kernel_series\"/" \
        -e "/^PACKAGE_VERSION=/a BUILD_EXCLUSIVE_KERNEL=\"^${kernel_series/./[.]}[.]\"" \
        "$snippets_dir/dkms.conf" > "$output_dir/dkms.conf"
    series_dir="$patchset_dir/patches/linux-$kernel_series"
    while IFS= read -r patch_file; do
        [[ -n $patch_file && $patch_file != \#* ]] || continue
        patch --directory "$output_dir" --strip=1 --batch --fuzz=0 < "$series_dir/$patch_file"
    done < "$series_dir/series"
    printf 'Prepared VKMS for Linux %s at %s\n' "$kernel_series" "$output_dir"
    printf 'Build with: make -C %q KDIR=/path/to/matching/kernel/build\n' "$output_dir"

# use insmod to temporarily load patched vkms
[group('vkms')]
vkms-insmod:
    make -C {{ VKMS_SRC }} -s
    sudo modprobe -r vkms
    sudo insmod {{ VKMS_SRC }}/vkms.ko create_default_dev=0

# Register a prepared patchset without resetting displays or reloading VKMS.
[group('vkms')]
vkms-dkms version="":
    #!/usr/bin/env bash
    set -euo pipefail
    source_dir='{{ VKMS_SRC }}'
    requested_version='{{ version }}'
    if [[ -z $source_dir ]]; then
        echo 'Set VKMS_SRC to your prepared module source directory:' >&2
        echo '  VKMS_SRC=path/to/vkms-edid-dkms just vkms-dkms' >&2
        exit 2
    fi
    source_version=$(sed -n 's/^PACKAGE_VERSION="\([^"]*\)"$/\1/p' "$source_dir/dkms.conf")
    case "$source_version" in 0.1|0.2|0.3-7.1|0.3-7.2) ;; *) echo 'Unrecognized DKMS source version' >&2; exit 2 ;; esac
    if [[ -n $requested_version && $requested_version != "$source_version" ]]; then
        echo "Requested version does not match $source_dir/dkms.conf" >&2
        exit 2
    fi
    destination="/usr/src/vkms-edid-$source_version"
    if ! sudo test ! -e "$destination" || ! sudo test ! -L "$destination"; then
        echo "Cannot create $destination without overwriting existing state. See the migration guide." >&2
        exit 1
    fi
    sudo cp -a "$source_dir" "$destination"
    sudo dkms add -m vkms-edid -v "$source_version"
    sudo dkms install -m vkms-edid -v "$source_version" -k "$(uname -r)"

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

# fauxput dev harness

Incus-managed Fedora 44 test VMs for fauxput.

Only use disposable hosts for these tests. They change display configuration and can disrupt the desktop session.

## Prerequisites

- `incus`, `just`
- `$VKMS_SRC` in `.envrc` pointing at a local `vkms-edid-dkms` checkout

## Flavors

- `gnome` — Mutter + GDM auto-login + Sunshine (end-to-end streaming)
- `kde` - KWin + Plasma
- `sway` — Sway + waybar + wdisplays
- `cosmic` - COSMIC desktop

## Commands

```sh
just dev vm-up <flavor>            # init + provision + snapshot
just dev vm-shell <flavor>         # shell as test user
just dev vm-reset <flavor>         # restore to `ready` snapshot
just dev vm-install <flavor>       # push freshly-built fauxput binary
just dev vm-vkms-rebuild <flavor>   # rebuild DKMS after $VKMS_SRC changes
just dev vm-down <flavor>          # delete the VM
```

Flavor defaults to `gnome`.

## wlr backend

Build with `cargo build --release --features wlr`. HDR, primary-output selection, and mirrored layouts are not supported yet.

See [COSMIC hotplug limitations](../docs/src/troubleshooting.md#cosmic-crashes-or-freezes-when-changing-displays).

## Display lifecycle tests

Only run these tests on a disposable host. They change display configuration and deliberately corrupt fauxput's runtime state. Do not run them on your desktop or streaming host, or in parallel.

Requires `just`, `bats`, `jq`, patched VKMS, and a GNOME Wayland session. Run from the fauxput checkout inside the VM.

```bash
FAUXPUT_TEST_DISPOSABLE=1 just dev test-gnome
# Run one test:
FAUXPUT_TEST_DISPOSABLE=1 just dev test-gnome --filter 'ten up/down'
```

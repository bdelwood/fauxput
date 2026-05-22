# fauxput dev harness

Incus-managed Fedora test VMs for fauxput.

## Prerequisites

- `incus`, `just`
- `$VKMS_SRC` in `.envrc` pointing at a local `vkms-edid-dkms` checkout

## Flavors

- `gnome` — Mutter + GDM auto-login + Sunshine (end-to-end streaming)
- `sway` — Sway + waybar + wdisplays

## Commands

```sh
just vm-up <flavor>            # init + provision + snapshot
just vm-shell <flavor>         # ssh as test user
just vm-reset <flavor>         # restore to `ready` snapshot
just vm-install <flavor>       # push freshly-built fauxput binary
just vm-vkms-rebuild <flavor>  # rebuild DKMS after $VKMS_SRC changes
just vm-down <flavor>          # delete the VM
```

Flavor defaults to `gnome`.

## Display lifecycle tests

Only run these tests on a disposable host. They change display configuration and deliberately corrupt fauxput's runtime state. Do not run them on your desktop or streaming host, or in parallel.

Requires `bats`, `jq`, patched VKMS, and a GNOME Wayland session.

```bash
FAUXPUT_TEST_DISPOSABLE=1 bash /mnt/fauxput-src/dev/bats.sh
# Run one test:
FAUXPUT_TEST_DISPOSABLE=1 bash /mnt/fauxput-src/dev/bats.sh --filter 'ten up/down'
```

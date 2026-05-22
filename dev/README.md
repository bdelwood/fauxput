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

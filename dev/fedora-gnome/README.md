# Fedora + GNOME test VM

Incus-managed Fedora 43 VM with GNOME, GDM auto-login, patched `vkms-edid-dkms`, and Sunshine pre-installed. For manually testing fauxput's GNOME adapter and end-to-end streaming.

`$VKMS_SRC` must point at a local vkms-edid-dkms checkout (set in `.envrc`).

## Display lifecycle tests

Only run these tests on a disposable host. They change display configuration and deliberately corrupt fauxput's runtime state. Do not run them on your desktop or streaming host, or in parallel.

Requires `bats`, `jq`, patched VKMS, and a GNOME Wayland session.

```bash
FAUXPUT_TEST_DISPOSABLE=1 bash /mnt/fauxput-src/dev/bats.sh
# Run one test:
FAUXPUT_TEST_DISPOSABLE=1 bash /mnt/fauxput-src/dev/bats.sh --filter 'ten up/down'
```

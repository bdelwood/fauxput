# Sunshine integration scripts

Drop-in scripts to wire fauxput into Sunshine's `prep-cmd` hook. See [the docs](../docs/src/streaming-setup.md) for the full walkthrough.

- `sunshine-fauxput-up.sh` / `sunshine-fauxput-down.sh` — Sunshine `prep-cmd` hooks that bring up a fauxput head matching the client's resolution and tear it down on disconnect.
- `sunshine-apps.json.example` — example config snippet for `~/.config/sunshine/apps.json`.

When fauxput is installed from the PKGBUILD, these scripts are at `/usr/share/fauxput/`. Reference them from your apps.json directly.

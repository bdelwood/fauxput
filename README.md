# fauxput


[![CI status][ci-img]][ci-url]
[![Documentation][doc-img]][doc-url]
[![AUR version][aur-img]][aur-url]
[![License][license-img]][license-url]

[ci-img]: https://img.shields.io/github/actions/workflow/status/bdelwood/fauxput/ci.yaml?branch=master&style=flat-square&label=CI
[ci-url]: https://github.com/bdelwood/fauxput/actions/workflows/ci.yaml
[doc-img]: https://img.shields.io/badge/docs-fauxput-4d76ae?style=flat-square
[doc-url]: https://bdelwood.github.io/fauxput/
[aur-img]: https://img.shields.io/aur/version/fauxput?style=flat-square&label=AUR
[aur-url]: https://aur.archlinux.org/packages/fauxput
[license-img]: https://img.shields.io/badge/license-MIT-yellow?style=flat-square
[license-url]: https://github.com/bdelwood/fauxput/blob/master/LICENSE

A cli for managing virtual displays on Wayland. Designed as a general-purpose virtual-display manager that integrates well with streaming hosts (Sunshine, Steam Remote Play).

## But why?

I was super annoyed that resolutions weren't being dynamically set by Sunshine clients. Luckily, with [recent vkms work to support virtual EDID profiles](https://indico.freedesktop.org/event/10/contributions/448/), it's now possible to configure virtual displays properly. Sunshine recently added support for an xdg-portal-based capture backend, which opens up wider DE support. fauxput plumbs the two together: a Moonlight client at any resolution $\rightarrow$ a vkms connector created at exactly that resolution $\rightarrow$ portal capture streams it back.

## Installation

### Prerequisites

- Linux kernel $\geq$ 7.0 with `vkms`
- A patched `vkms` with the EDID configfs interface. See the [docs](docs/src/kernel-dependency.md) for details.
- For streaming integration: Sunshine.
    - Recommend using Sunshine $\geq$ 2026.4 which has a new portal capture backend that should work on any compositor that supports `xdg-desktop-portal`.

### System Dependencies

- `libcap`
- `libxcvt`
- `util-linux`
- `wayland`


### Build from source

```bash
cargo build --release
```


or with `cargo install`:

```bash
cargo install --path .
```


Optional, if you want to use with Sunshine:
```bash
sudo setcap cap_dac_override+ep path/to/binary/fauxput
```

### AUR

```bash
yay -S fauxput
```


## Quickstart


Create a virtual display, make it the compositor's primary, and disables real outputs. Disabling real outputs can be useful to force the compositor to put newly launched windows onto the virtual display:

```bash
fauxput up --width 1920 --height 1080 --fps 60 --primary --disable-real-outputs
```

Check virtual display status:

```bash
fauxput status
```
Undo setup and tear everything down:
```bash
fauxput down
```

Force clean :
```bash
fauxput reset --yes
```

## DE / Sunshine support

| Desktop | Supported version | Status |
|---|---|---|
| KDE Plasma (kwin) | $\geq$ 6.2 | ✓ |
| GNOME (Mutter) | $\geq$ 3.36 | ✓  |
| wlroots (Sway, Hyprland, ...) | TBD | planned |


Sunshine $\geq$ 2026.4 portal capture works on any of the above once the adapter is in place.

## Documentation

- [Streaming setup walkthrough](docs/src/streaming-setup.md) — wire fauxput into Sunshine + Moonlight
- [Troubleshooting + working Sunshine recipe](docs/src/troubleshooting.md.md)
- [Kernel-side dependency](docs/src/kernel-dependency.md) — the `vkms-edid-dkms` patch series


## TODO

- [ ] Additional compositor support
    - [x] Mutter
    - [ ] wlroots family
- [ ] HDR & VRR
- [ ] Preset profiles

## License

MIT. See [LICENSE](LICENSE).

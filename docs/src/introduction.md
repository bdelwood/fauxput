# Introduction

To run fauxput, you'll need:

- **Linux kernel ≥ 7.0** with the [vkms](https://docs.kernel.org/gpu/vkms.html) driver enabled.
- **Patched `vkms.ko`** — the [`vkms-edid-dkms`](./kernel-dependency.md) out-of-tree module, until the EDID-via-configfs patches land in mainline.
- **A Wayland compositor** with `wlr-output-management` (Sway, Hyprland, river, Wayfire, COSMIC, etc) or `kde-output-management-v2` (KWin / Plasma 6+).

The [kernel dependency guide](./kernel-dependency.md) covers the OOT module build and install. For the API reference, see the [generated rustdoc](https://bdelwood.github.io/fauxput/rustdoc/fauxput/).

## Project links

- [GitHub repository](https://github.com/bdelwood/fauxput)
- [Crates.io](https://crates.io/crates/fauxput)
- [Issue tracker](https://github.com/bdelwood/fauxput/issues)

## License

MIT.

# Introduction

To run fauxput, you'll need:

- **Linux kernel ≥ 7.0** with the [vkms](https://docs.kernel.org/gpu/vkms.html) driver enabled.
- **Patched `vkms`** providing the EDID-via-configfs interface. Until those patches land in mainline, install the [`vkms-edid-dkms`](./kernel-dependency.md) out-of-tree module.
- **A Wayland compositor**: KDE Plasma or GNOME. Support for the `wlroots` family is planned.

The [kernel dependency guide](./kernel-dependency.md) covers the OOT module build and install. For the API reference, see the [generated rustdoc](https://bdelwood.github.io/fauxput/rustdoc/fauxput/).

## Project links

- [GitHub repository](https://github.com/bdelwood/fauxput)
- [Crates.io](https://crates.io/crates/fauxput)
- [Issue tracker](https://github.com/bdelwood/fauxput/issues)

## License

MIT.

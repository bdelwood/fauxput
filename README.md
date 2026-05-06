# fauxput


[![CI status][ci-img]][ci-url]
[![Documentation][doc-img]][doc-url]
[![AUR version][aur-img]][aur-url]
[![License][license-img]][license-url]

[ci-img]: https://img.shields.io/github/actions/workflow/status/bdelwood/fauxput/ci.yaml?branch=main&style=flat-square&label=CI
[ci-url]: https://github.com/bdelwood/fauxput/actions/workflows/ci.yaml
[doc-img]: https://img.shields.io/badge/docs-fauxput-4d76ae?style=flat-square
[doc-url]: https://bdelwood.github.io/fauxput/
[aur-img]: https://img.shields.io/aur/version/fauxput?style=flat-square&label=AUR
[aur-url]: https://aur.archlinux.org/packages/fauxput
[license-img]: https://img.shields.io/badge/license-MIT-yellow?style=flat-square
[license-url]: https://github.com/bdelwood/fauxput/blob/main/LICENSE

A cli for managing virtual displays on Wayland. Designed as a general-purpose virtual-display manager that integrates well with streaming hosts (Sunshine, Steam Remote Play).

## But why?

I was super annoyed that resolutions weren't being dynamically set by Sunshine clients. Luckily, with recent vkms work to support virtual EDID profiles, it's now possible to configure virtual displays properly. Sunshine recently added support for an xdg-portal-based capture backend, which opens up wider DE support. fauxput plumbs the two together: a Moonlight client at any resolution $\rightarrow$ a vkms connector created at exactly that resolution $\rightarrow$ portal capture streams it back.

## License

MIT. See [LICENSE](LICENSE).

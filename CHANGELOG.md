# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0](https://github.com/bdelwood/fauxput/releases/tag/v0.1.0) - 2026-05-10

### Added

- add colors to cli to feel fancy
- add reset to cli, make status more useful
- wire up cli to actual orchestration functions
- add top level orchestration module
- add state management for active instances
- complete compositor implementation for KDE.
- add initial cli
- add partial implementation for  kde backend
- implement more consistent feature handling.
- add basic wayland scaffolding and traits
- add logging to vkms backend mod
- add configfs-vkms backend
- attempt first pass at EDID construction
- implement basic timing module wrapping libxcvt

### Fixed

- set permissions on correct obj

### Other

- backfill CHANGELOG for initial release
- cleanup
- improve comments throughout
- template pkgver for aur releases
- make it so docs workflow runs on PRs too
- add release-plz in prep for release
- change deps version constraints to major-only
- Update readme and add troubleshooting and Sunshine docs
- add basic PKGBUILD and Sunshine scripts
- minor changes to keep clippy happy
- consolidate tests and make outputplan's builder interface more sane
- use indexmap to slightly simplify priority setting
- add some simple unit tests for kwin module
- add more tests for builder/capability check
- add ci and docs workflows
- document required vkms kernel patches
- implement some edid tests, sprinkle in some logs
- add basic justfile
- init

## [0.5.0] - 2026-05-10

### Bug Fixes
- Set permissions on correct obj [`bc1caba`](https://github.com/bdelwood/fauxput/commit/bc1caba2c9807b3670b4652469001eaa4251bda3)

### Build
- Add basic PKGBUILD and Sunshine scripts [`8c4054d`](https://github.com/bdelwood/fauxput/commit/8c4054d3ee2764f2b6898f96d4cf47815f50eaec)

### CI
- Make it so docs workflow runs on PRs too [`0e6e3b8`](https://github.com/bdelwood/fauxput/commit/0e6e3b85a399961dd9212df0fa4a7533d8b32721)
- Add ci and docs workflows [`e99a432`](https://github.com/bdelwood/fauxput/commit/e99a4324c460958f3939b5a72b986c4218af2b5f) by @bdelwood

### Documentation
- Cleanup [`e4d9f9a`](https://github.com/bdelwood/fauxput/commit/e4d9f9a730142787a681331ccac84790b06f204b)
- Improve comments throughout [`d320be6`](https://github.com/bdelwood/fauxput/commit/d320be6c78fcb17c7b6976ee22754727f9d4c15d)
- Update readme and add troubleshooting and Sunshine docs [`10bc97a`](https://github.com/bdelwood/fauxput/commit/10bc97abdf2e5211e4b254d13d28837d3da9ed6e)
- Document required vkms kernel patches [`52270df`](https://github.com/bdelwood/fauxput/commit/52270dfc8ebc5fbd800d6519727f35c27a4a58a9) by @bdelwood

### Features
- Add colors to cli to feel fancy [`584eba9`](https://github.com/bdelwood/fauxput/commit/584eba9ace213b6fdbdf1ff3cc1347251764daee)
- Add reset to cli, make status more useful [`b584d0d`](https://github.com/bdelwood/fauxput/commit/b584d0df19e5bd28e9c61651ea627726e13f2595)
- Wire up cli to actual orchestration functions [`f8eebf3`](https://github.com/bdelwood/fauxput/commit/f8eebf3db37a54946de378b8a7d9d040d6f91aa9)
- Add top level orchestration module [`254ca43`](https://github.com/bdelwood/fauxput/commit/254ca435256b0af5c259927f2cf07e54700e2459)
- Add state management for active instances [`b250c32`](https://github.com/bdelwood/fauxput/commit/b250c32f2214d0836a8b56198bbde140f69d1c8e)
- Complete compositor implementation for KDE. [`6ddab01`](https://github.com/bdelwood/fauxput/commit/6ddab0189695c8eaab89dd86b75602945ef5ab41)
- Add initial cli [`1c4ff14`](https://github.com/bdelwood/fauxput/commit/1c4ff145e2a202704a2c0351233c0fba176afb68)
- Add partial implementation for  kde backend [`6284c4e`](https://github.com/bdelwood/fauxput/commit/6284c4e87dfeae0d52f5d0eefb8e69aa20557766)
- Implement more consistent feature handling. [`9cb2efb`](https://github.com/bdelwood/fauxput/commit/9cb2efb7aa6089244fb8b08d5ce5fa51057389a3)
- Add basic wayland scaffolding and traits [`fd8799e`](https://github.com/bdelwood/fauxput/commit/fd8799e9dc729505dd274df72a902499faaca950)
- Add logging to vkms backend mod [`2028f16`](https://github.com/bdelwood/fauxput/commit/2028f161e182d53610355bc59fd71b0bd0f7d59d) by @bdelwood
- Add configfs-vkms backend [`fb96092`](https://github.com/bdelwood/fauxput/commit/fb96092c2a9f2317ac4fff73bb68af9c91b6effd) by @bdelwood
- Attempt first pass at EDID construction [`871f818`](https://github.com/bdelwood/fauxput/commit/871f818368608b6fa23d12e36ac9566136830f2f) by @bdelwood
- Implement basic timing module wrapping libxcvt [`2a2ae7a`](https://github.com/bdelwood/fauxput/commit/2a2ae7aadbc16691668492859ab5160fa061aa6f) by @bdelwood

### Miscellaneous
- Backfill CHANGELOG for initial release [`fffece2`](https://github.com/bdelwood/fauxput/commit/fffece2e3f593b93f082cef9dcc15a88b6feac2e)
- Template pkgver for aur releases [`b4db5da`](https://github.com/bdelwood/fauxput/commit/b4db5dae7710c971d0304475ce062c39c6b9136c)
- Add release-plz in prep for release [`65c0ae9`](https://github.com/bdelwood/fauxput/commit/65c0ae9786b5e9106c21d85c2e330860f4906287)
- Change deps version constraints to major-only [`9d2a058`](https://github.com/bdelwood/fauxput/commit/9d2a0587aec488ce7db1a90bff72b288dbb9940d)
- Add basic justfile [`d5140a8`](https://github.com/bdelwood/fauxput/commit/d5140a8e0e5059ad1ee48286c93e6b7fa0aec720) by @bdelwood
- Init [`f72185c`](https://github.com/bdelwood/fauxput/commit/f72185c958cbf190d4c6338d0cde73fb24d47293) by @bdelwood

### Other
- Minor changes to keep clippy happy [`e8c263f`](https://github.com/bdelwood/fauxput/commit/e8c263f0ddba6ea6fe58575620793b4ba250f95e)

### Refactor
- Consolidate tests and make outputplan's builder interface more sane [`2ddef7c`](https://github.com/bdelwood/fauxput/commit/2ddef7c48fdcc237decc90416796cffcde180d99)
- Use indexmap to slightly simplify priority setting [`562b912`](https://github.com/bdelwood/fauxput/commit/562b912ce0a8fbe7c20a37e915649f5a0ebf9ef6)

### Tests
- Add some simple unit tests for kwin module [`dd2d583`](https://github.com/bdelwood/fauxput/commit/dd2d583054c154ad4bd4e1284ded21713743d29e)
- Add more tests for builder/capability check [`3ae06da`](https://github.com/bdelwood/fauxput/commit/3ae06da2e57031ea97dcba1eb5a46ef74482c52e)
- Implement some edid tests, sprinkle in some logs [`156c329`](https://github.com/bdelwood/fauxput/commit/156c3297eb884b130779e6fcb1e64055449b2ea8) by @bdelwood


<!-- generated by git-cliff -->

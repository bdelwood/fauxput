# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-05-10

### Bug Fixes
- Set permissions on correct obj [`bc1caba`](https://github.com/bdelwood/fauxput/commit/bc1caba2c9807b3670b4652469001eaa4251bda3) by @bdelwood

### Build
- Add basic PKGBUILD and Sunshine scripts [`2ac702b`](https://github.com/bdelwood/fauxput/commit/2ac702b343cee770af38301f6835b0b89522f762)

### CI
- Make it so docs workflow runs on PRs too [`54b154e`](https://github.com/bdelwood/fauxput/commit/54b154ed411bb644ee5a0dff2618941c2f729fb9)
- Add ci and docs workflows [`e99a432`](https://github.com/bdelwood/fauxput/commit/e99a4324c460958f3939b5a72b986c4218af2b5f) by @bdelwood

### Documentation
- Cleanup [`a8a0f7a`](https://github.com/bdelwood/fauxput/commit/a8a0f7a2668429af9e75746964b74bddfaaff436)
- Improve comments throughout [`6952356`](https://github.com/bdelwood/fauxput/commit/695235659e4ffd55a940e5133a3aec29a350768a)
- Update readme and add troubleshooting and Sunshine docs [`3ae723a`](https://github.com/bdelwood/fauxput/commit/3ae723a182a992807345f8d3e3126854e858bb44)
- Document required vkms kernel patches [`52270df`](https://github.com/bdelwood/fauxput/commit/52270dfc8ebc5fbd800d6519727f35c27a4a58a9) by @bdelwood

### Features
- Add colors to cli to feel fancy [`4848534`](https://github.com/bdelwood/fauxput/commit/484853417c4a1d7a5433d6ff9aa6982947263718)
- Add reset to cli, make status more useful [`b584d0d`](https://github.com/bdelwood/fauxput/commit/b584d0df19e5bd28e9c61651ea627726e13f2595) by @bdelwood
- Wire up cli to actual orchestration functions [`f8eebf3`](https://github.com/bdelwood/fauxput/commit/f8eebf3db37a54946de378b8a7d9d040d6f91aa9) by @bdelwood
- Add top level orchestration module [`254ca43`](https://github.com/bdelwood/fauxput/commit/254ca435256b0af5c259927f2cf07e54700e2459) by @bdelwood
- Add state management for active instances [`b250c32`](https://github.com/bdelwood/fauxput/commit/b250c32f2214d0836a8b56198bbde140f69d1c8e) by @bdelwood
- Complete compositor implementation for KDE. [`6ddab01`](https://github.com/bdelwood/fauxput/commit/6ddab0189695c8eaab89dd86b75602945ef5ab41) by @bdelwood
- Add initial cli [`1c4ff14`](https://github.com/bdelwood/fauxput/commit/1c4ff145e2a202704a2c0351233c0fba176afb68) by @bdelwood
- Add partial implementation for  kde backend [`6284c4e`](https://github.com/bdelwood/fauxput/commit/6284c4e87dfeae0d52f5d0eefb8e69aa20557766) by @bdelwood
- Implement more consistent feature handling. [`9cb2efb`](https://github.com/bdelwood/fauxput/commit/9cb2efb7aa6089244fb8b08d5ce5fa51057389a3) by @bdelwood
- Add basic wayland scaffolding and traits [`fd8799e`](https://github.com/bdelwood/fauxput/commit/fd8799e9dc729505dd274df72a902499faaca950) by @bdelwood
- Add logging to vkms backend mod [`2028f16`](https://github.com/bdelwood/fauxput/commit/2028f161e182d53610355bc59fd71b0bd0f7d59d) by @bdelwood
- Add configfs-vkms backend [`fb96092`](https://github.com/bdelwood/fauxput/commit/fb96092c2a9f2317ac4fff73bb68af9c91b6effd) by @bdelwood
- Attempt first pass at EDID construction [`871f818`](https://github.com/bdelwood/fauxput/commit/871f818368608b6fa23d12e36ac9566136830f2f) by @bdelwood
- Implement basic timing module wrapping libxcvt [`2a2ae7a`](https://github.com/bdelwood/fauxput/commit/2a2ae7aadbc16691668492859ab5160fa061aa6f) by @bdelwood

### Miscellaneous
- Backfill CHANGELOG for initial release [`0677c38`](https://github.com/bdelwood/fauxput/commit/0677c38a4d6183092c9edeeef21b36144fcea278)
- Template pkgver for aur releases [`8a0888f`](https://github.com/bdelwood/fauxput/commit/8a0888f4bcc90f02e676d2cf9ef7933c678570e6)
- Add release-plz in prep for release [`4660895`](https://github.com/bdelwood/fauxput/commit/46608957d4a758aa449a9ae62a7caf3867e80df8)
- Change deps version constraints to major-only [`b55d88f`](https://github.com/bdelwood/fauxput/commit/b55d88ff7eb3706c9604fe8003ad6672f7c52e21)
- Add basic justfile [`d5140a8`](https://github.com/bdelwood/fauxput/commit/d5140a8e0e5059ad1ee48286c93e6b7fa0aec720) by @bdelwood
- Init [`f72185c`](https://github.com/bdelwood/fauxput/commit/f72185c958cbf190d4c6338d0cde73fb24d47293) by @bdelwood

### Refactor
- Consolidate tests and make outputplan's builder interface more sane [`2ddef7c`](https://github.com/bdelwood/fauxput/commit/2ddef7c48fdcc237decc90416796cffcde180d99) by @bdelwood
- Use indexmap to slightly simplify priority setting [`562b912`](https://github.com/bdelwood/fauxput/commit/562b912ce0a8fbe7c20a37e915649f5a0ebf9ef6) by @bdelwood

### Styling
- Keep clippy happy [`fe196ba`](https://github.com/bdelwood/fauxput/commit/fe196ba65d5d9cb6c835648fe480eaa4fb98ff18)

### Tests
- Add some simple unit tests for kwin module [`dd2d583`](https://github.com/bdelwood/fauxput/commit/dd2d583054c154ad4bd4e1284ded21713743d29e) by @bdelwood
- Add more tests for builder/capability check [`3ae06da`](https://github.com/bdelwood/fauxput/commit/3ae06da2e57031ea97dcba1eb5a46ef74482c52e) by @bdelwood
- Implement some edid tests, sprinkle in some logs [`156c329`](https://github.com/bdelwood/fauxput/commit/156c3297eb884b130779e6fcb1e64055449b2ea8) by @bdelwood


<!-- generated by git-cliff -->

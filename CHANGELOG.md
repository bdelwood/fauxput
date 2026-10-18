# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0] - 2026-10-18

### Bug Fixes
- Identify outputs through device connectors [`0451fdf`](https://github.com/bdelwood/fauxput/commit/0451fdf64e12a6908b8dec63cc5b4e76c40786a2)
- Propagate baseline snapshot errors [`f927496`](https://github.com/bdelwood/fauxput/commit/f927496124955d02b59f0ce3faca47b7a5c645f7)
- Generate compliant CTA extension blocks [`7b050ba`](https://github.com/bdelwood/fauxput/commit/7b050ba9663db58f43e07894f61329f880ee2e34)
- Reconcile HDR and wide color gamut independently [`8f0a4bd`](https://github.com/bdelwood/fauxput/commit/8f0a4bdd6a2bd00d15768ea3607df37ce391c9a6)
- Clarify the detailed timing clock limit [`13c4862`](https://github.com/bdelwood/fauxput/commit/13c4862a4baedc4a89e8cbb5d8d587ac67e5a1aa)
- Preserve output layout across setup and teardown [`7edb62d`](https://github.com/bdelwood/fauxput/commit/7edb62d36e1dd56f3152f3556e49c21a208e39f9)
- Await disconnection before destroying displays [`a12ee36`](https://github.com/bdelwood/fauxput/commit/a12ee36aa7e869bcef40ef73ce803656c03d18f7)
- Connect outputs after registering the GPU [`a846639`](https://github.com/bdelwood/fauxput/commit/a8466390b6e0ba7162dc73b3a00b750eac5d5032)
- Discard removed outputs and stale mode events [`e63a275`](https://github.com/bdelwood/fauxput/commit/e63a275c99375c21152c83e8506374e58c97fdef)
- Use correct "ApplyMonitorsConfig" signature for HDR color-mode on Mutter [`e215144`](https://github.com/bdelwood/fauxput/commit/e2151445084aff213c65385f2e628604f50f628b)
- Align `down` and `reset` teardown procedures [`587192f`](https://github.com/bdelwood/fauxput/commit/587192fbf3b8d265ff75453ec910f3e35b2c7fd0)
- Improve mutter teardown heuristics [`5c371d3`](https://github.com/bdelwood/fauxput/commit/5c371d323cba0af9cf3fc1b468d073c5ab59ef7b)
- Tell kernel that we're DP when requesting HDR [`1b8d3e5`](https://github.com/bdelwood/fauxput/commit/1b8d3e5938d8a63521f16b0903f3b6fd83a7c6d9)
- Better hdr error in cli [`6f97941`](https://github.com/bdelwood/fauxput/commit/6f9794182c0836c15cdfab9a2c2635898c263300)
- Fix pkgbuild dependencies [`052a99d`](https://github.com/bdelwood/fauxput/commit/052a99d4553f142d9ffb2a96f49a7eb7fd14ecd3)
- Fix readme link [`5b145a1`](https://github.com/bdelwood/fauxput/commit/5b145a17f7c2a0ae54c927d586751d04c924cead)
- Skip mutter hotplug workaround for kde [`aaac380`](https://github.com/bdelwood/fauxput/commit/aaac3805aeb06114ba8da5c63bb0c6d3605411f4)
- Place virtual head at origin when using --disable-real-outputs [`f3e8387`](https://github.com/bdelwood/fauxput/commit/f3e83872e3385c21e9b3ecf285df5e51962d06d6)

### Documentation
- Document HDR streaming setup and supply capture patches [`cc0901b`](https://github.com/bdelwood/fauxput/commit/cc0901bb1a04a0839ab766c1b4a464ec255209b9)
- Document GPU hotplug limitations and workaround [`1f3a253`](https://github.com/bdelwood/fauxput/commit/1f3a253956ac52af9283853b3e7d75db9d7ee301)

### Features
- Model and preserve compositor color modes [`4656d7c`](https://github.com/bdelwood/fauxput/commit/4656d7cd04ce69ad2c55868349a5cd4a966c88dc)
- Add compositor-side HDR toggling, so it doesn't have to be done manually per head now [`983a477`](https://github.com/bdelwood/fauxput/commit/983a47761e3c379b8f4bf55e984ad117a5675364)
- Log sunshine output [`e12d0ce`](https://github.com/bdelwood/fauxput/commit/e12d0ce9d5b5b5261007d8ad5cb923a9365e3338)
- Add Plasma >=6.7 support [`b5bca4e`](https://github.com/bdelwood/fauxput/commit/b5bca4e18faebe0a87c80a3e11e8b0368e10bfc0)
- Add initial HDR support [`6c4d11b`](https://github.com/bdelwood/fauxput/commit/6c4d11b2d5362a4399b741fe70166733b7d4e3b6)

### Miscellaneous
- Clarify dependency and feature TODOs [`80f45b3`](https://github.com/bdelwood/fauxput/commit/80f45b365e7811094995f416a385b5bfe53c499e)
- Lockfile maint [`640554b`](https://github.com/bdelwood/fauxput/commit/640554bc566058b748862486ba5e3328733006eb)
- Add docs for with support VKMS builds on Linux 7.1 and 7.2 [`ab75410`](https://github.com/bdelwood/fauxput/commit/ab7541018ee0f00653b213d28e984946ee648f55)

### Other
- Merge pull request #2 from Pesc0/master [`63255d8`](https://github.com/bdelwood/fauxput/commit/63255d89cd2ac6e00ae2ed215d9fd0f7703c0aae)
- Update vkms build instructions [`6581fd7`](https://github.com/bdelwood/fauxput/commit/6581fd7228afcf54ab7ce03b0636b426908f0ed6)

### Refactor
- Share output state waits [`adc9335`](https://github.com/bdelwood/fauxput/commit/adc9335c8cb0fb30ab071357a9280615c811db00)

### Tests
- Cover output removal, layout restoration, and HDR modes [`1acae75`](https://github.com/bdelwood/fauxput/commit/1acae75781583cdeba501e61faf41ff530ea4c39)
- Update disposable GNOME VM workflow and diagnostics [`61617aa`](https://github.com/bdelwood/fauxput/commit/61617aa3c8b95195a091e5468bd0b4d8130e0b0e)
- Add sway test vm [`83a41bd`](https://github.com/bdelwood/fauxput/commit/83a41bd3acfd1e442df757aea60db6551ca5909e)
- Add guarded GNOME display lifecycle torture suite [`e80f44c`](https://github.com/bdelwood/fauxput/commit/e80f44ce6f2a34d4d3f3763be8928eac2675fa1a)

## [0.6.1] - 2026-05-12

### CI
- Replace release workflow with on-release-created binary upload [`13ecef4`](https://github.com/bdelwood/fauxput/commit/13ecef410640cd40daa325410dd3ce4dfbe0ad3c)

### Miscellaneous
- Drop release-plz; manual git-cliff + tag cuts going forward [`7f88336`](https://github.com/bdelwood/fauxput/commit/7f88336bb9205cc7820d6b93720930d6dc6eea37)

## [0.6.0] - 2026-05-12

### Bug Fixes
- Add conflicts [`cd9df8b`](https://github.com/bdelwood/fauxput/commit/cd9df8be604ab32e8c459dd090ed0d40a4bf690d) by @bdelwood

### Build
- Gate compositor adapters behind cargo features [`0d68d06`](https://github.com/bdelwood/fauxput/commit/0d68d06fe9f6cddf9c998a488741736292068453) by @bdelwood

### Documentation
- Update readme [`4f28dd4`](https://github.com/bdelwood/fauxput/commit/4f28dd4c4eb834a5773e3bc4bf973dbe12e7196b)

### Features
- Finish mutter support [`4bfa6c4`](https://github.com/bdelwood/fauxput/commit/4bfa6c478c6d4b73794cdce218dddb8e20e7372d) by @bdelwood
- Complete gnome snapshot adapter [`b0526bc`](https://github.com/bdelwood/fauxput/commit/b0526bc8a04da2c047dfe334a6e5b2bea6a80ae1) by @bdelwood
- Read gnome monitor state from DisplayConfig [`23b4e28`](https://github.com/bdelwood/fauxput/commit/23b4e28ad4229a61b84c79bb878a5059cc5a71d4) by @bdelwood
- Add Mutter DisplayConfig dbus proxy [`b48a901`](https://github.com/bdelwood/fauxput/commit/b48a901f06bd3b30cadced62e87d9c508def5aa4) by @bdelwood
- Include pre-built binaries in release [`f416cd7`](https://github.com/bdelwood/fauxput/commit/f416cd78b4efc591ba21a24b304ecca936878999) by @bdelwood

### Miscellaneous
- Add pre-commit to stop making stupid mistakes that anger clippy [`d990164`](https://github.com/bdelwood/fauxput/commit/d99016429002aac02dd91d50cfd8c5a774144cdd)
- Add Fedora+GNOME VM dev testing setup [`b930a4b`](https://github.com/bdelwood/fauxput/commit/b930a4b294c1f0a44fb3b6ff341ff130da33b472) by @bdelwood
- Add keywords to cargo [`529b9fa`](https://github.com/bdelwood/fauxput/commit/529b9fa3a7e4b6c87696973c0b07da5e08bba1e7) by @bdelwood

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


[0.7.0]: https://github.com/bdelwood/fauxput/compare/v0.6.1...v0.7.0

<!-- generated by git-cliff -->

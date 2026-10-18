# HDR on Plasma

`fauxput up --hdr` creates an HDR virtual display. To stream it, install the components below and enable HDR in Moonlight.

Run the build commands below from fauxput's root.

## Virtual display

Use Plasma 6.8 or newer. Install fauxput using the [installation instructions](https://github.com/bdelwood/fauxput#installation) and the patched VKMS module using the [kernel guide](./kernel-dependency.md). Check that an HDR display can be created:

```bash
fauxput up --width 3840 --height 2160 --fps 60 --primary --hdr
kscreen-doctor -o
```

Check that the fauxput output has HDR and wide color gamut enabled, then run `fauxput down` before setting up Sunshine. Omit `--hdr` for an SDR display.

## KDE / KWin

> [!NOTE]
> HDR output should work on Plasma ≥6.2. HDR streaming requires Plasma ≥6.7 with the capture patches described below. This guide targets Plasma ≥6.8.

Install PipeWire ≥1.5.81 and its development headers. Build and install KWin for your Plasma version with these patches, applying the upstream changes first.

### Upstream MRs

- [!8938: more than eight bits per color](https://invent.kde.org/plasma/kwin/-/merge_requests/8938)
- [!8293: HDR screencasts](https://invent.kde.org/plasma/kwin/-/merge_requests/8293)

The supplied MR patches are rebased onto KWin 6.8.0:

```bash
git clone --branch v6.8.0 https://invent.kde.org/plasma/kwin.git kwin
git -C kwin switch -c hdr-6.8
git -C kwin am "$PWD"/contrib/patches/kwin/*-upstream-*.patch
```

### Custom patches

Then apply our patches:

```bash
git -C kwin am "$PWD"/contrib/patches/kwin/*-custom-*.patch
```

Build and install the patched checkout using the [KDE build instructions](https://develop.kde.org/docs/getting-started/building/kde-builder-compile/). With kde-builder, use its configured KWin source directory and `kde-builder --no-src --no-include-dependencies kwin` to keep the patches applied.

## Sunshine

Use Sunshine ≥2026.516.143833.

### Custom patches

The supplied HDR patch targets Sunshine 2026.914.233613:

```bash
git clone --branch v2026.914.233613 --recurse-submodules \
    https://github.com/LizardByte/Sunshine.git sunshine
git -C sunshine am "$PWD"/contrib/patches/sunshine/*.patch
```

Build and install the patched checkout using the [Sunshine build instructions](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2building.html).

### Configure streaming

Follow the [streaming setup](./streaming-setup.md) to configure the supplied fauxput wrappers. Use Sunshine's portal capture backend:

```ini
capture = portal
```

Use an encoder that supports ten-bit HEVC or AV1. The supplied wrapper passes `--hdr` when the client requests HDR.

On first connection, set `FAUXPUT_KEEP_REAL=1` in the Sunshine app's environment so you can approve the portal chooser on a physical display. Select the fauxput display. After approval, you can remove that setting to disable physical outputs while streaming.

## Moonlight and verification

Enable HDR on the client's display and in Moonlight, then connect. Check `kscreen-doctor -o` for HDR on the fauxput output and Sunshine's log for HDR capture and ten-bit HEVC or AV1 encoding.

## HDR setup warning

`HDR didn't fully get enabled by the compositor` means fauxput could not confirm HDR on the new output. Run `kscreen-doctor -o` and check that the fauxput output has HDR and wide color gamut enabled. If HDR is unavailable, check that the patched VKMS module is loaded. Add `-vv` on the next setup for details; Sunshine wrapper logs are available with `journalctl -t fauxput-sunshine -b`.

## Color problems

If colors look washed out or oversaturated, check that the HDR settings on the host, in Moonlight, and on the client's display agree. Confirm the active stream's HDR mode in Sunshine's log.

If switching Moonlight to software decoding fixes the colors, investigate the client's hardware decoder and graphics driver too.

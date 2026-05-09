# Streaming setup

Wire fauxput into Sunshine + Moonlight so a Moonlight client connecting at, say, 2560×1440@120 gets a virtual display at exactly that resolution, configured automatically, and torn down on disconnect.

Install fauxput per the [README's Installation section](https://github.com/bdelwood/fauxput#installation). The PKGBUILD drops the Sunshine wrappers under `/usr/share/fauxput/`; for source builds, copy them from `contrib/`.

## Sunshine capture backends

Pick one in `~/.config/sunshine/sunshine.conf`:

- **Portal** (`capture = portal`) — recommended. DE-agnostic; goes through xdg-desktop-portal and PipeWire, so hardware acceleration (NVENC, VAAPI) works against the virtual display. First connect prompts a source-picker dialog from the active portal backend (e.g. xdg-desktop-portal-kde); select fauxput's output. Subsequent connects auto-restore. Available in Sunshine $\geq$ 2026.4.
- **KMS** (`capture = kms`) — legacy. Reads framebuffers directly through DRM; the vkms framebuffer lives in CPU memory, so encoders fall back to a GPU→RAM→GPU readback path with no hardware-accelerated capture.

## Configure Sunshine to use fauxputs

Add an app in Sunshine's web UI (the `Apps` tab) with the fauxput wrappers as `do` / `undo` prep-cmds: `sunshine-fauxput-{up,down}.sh`. See Sunshine's [App Examples](https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2app__examples.html) for details on how to configure. An example config is at `/usr/share/fauxput/sunshine-apps.json.example`.

The wrappers read `SUNSHINE_CLIENT_WIDTH/HEIGHT/FPS` and call `fauxput up --primary --disable-real-outputs` / `fauxput down`. Set `FAUXPUT_KEEP_REAL=1` in the app's env block to leave host outputs enabled while streaming.

## Verify

Connect from Moonlight at a non-host resolution. The stream should render at exactly that resolution; `fauxput status` shows the virtual display while connected and it disappears on disconnect.

## Issues

See [troubleshooting](./troubleshooting.md).

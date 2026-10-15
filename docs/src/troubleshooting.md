# Troubleshooting

Gotchyas that will break fauxput in non-obvious ways.

## GNOME stops detecting new displays

Mutter can keep removed VKMS devices in its GPU list ([GNOME/mutter#710](https://gitlab.gnome.org/GNOME/mutter/-/issues/710)). If its log says `Failed to hotplug secondary gpu` with `device already present`, log out and back in to clear that state. Repeated creation and removal can also crash the compositor.

<!-- TODO: Remove this workaround once Mutter handles GPU removal and reuse: https://gitlab.gnome.org/GNOME/mutter/-/issues/710 -->
Keep the fauxput display up between streaming connections to avoid repeated GPU hotplug. Create it once with `fauxput up`, omit the per-connection `up`/`down` commands, and run `fauxput down` when you're finished.

## SDDM with an X11 greeter results in logout on `fauxput down`

If you're running SDDM with an X11 greeter, `fauxput down` (or anything else that destroys a vkms instance) will kick you back to the login screen.

The greeter crashes on the DRM hot-unplug event, taking your session down as collateral.

To check:
```bash
loginctl show-session $(loginctl list-sessions --no-legend | awk '/greeter/{print $1}') -p Type
# Type=x11      > affected
# Type=wayland  > safe
```

The fix is to use SDDM in Wayland mode, or PLM.

---

## Sunshine portal-capture token doesn't survive an unclean shutdown

When using the Sunshine's portal backend, after a reboot, Moonlight reconnect re-prompts the portal chooser instead of silently restoring the previous fauxput selection.

If Sunshine shuts down uncleanly, the compositor will ask for a new token on the next connect.

---

## Pick the chooser display before `--disable-real-outputs`

The xdg-desktop-portal permission dialog appears on the **host** desktop. If `--disable-real-outputs` is enabled, there's nowhere to interact with the modal. On first connect, run `fauxput up` with just `--primary`, accept the permissions modal when it appears, and pick fauxput's virtual output. Subsequent connects auto-restore the choice without the dialog.

# Kernel dependency: vkms-edid-dkms

fauxput writes per-display EDID into `/sys/kernel/config/vkms/<inst>/connectors/0/edid`. That attribute doesn't exist in mainline Linux 7.0.x; it's part of Louis Chauvet's [VKMS configfs attributes patch series](https://lore.kernel.org/dri-devel/?q=s%3A%22vkms%3A+Introduce+multiple+configFS%22). Until upstream lands, fauxput needs a patched `vkms.ko`. The `fauxput` feature-detects the `edid` attribute and warns when it's missing; with the patched module loaded, requested resolutions take effect.

## Building the patched module

Nearly all of the patches modify only files under `drivers/gpu/drm/vkms/`. The other patches add helper functions in DRM core, but those helpers are only consumed by `vkms_config_show()` for debugfs output. An OOT shim header (`vkms_oot_shim.h`) replaces them with static stubs, so the patched module compiles against *installed* kernel headers, avoiding the need for a full kernel rebuild.

Prerequisites: `b4`, `git`, kernel headers for the running kernel (`linux-headers-$(uname -r)` on Debian/Ubuntu, `linux-headers` on Arch), `make`, a C compiler.

```bash
# Fetch a clean kernel
git clone --depth 1 --branch v7.0 \
    https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git \
    linux-vkms
cd linux-vkms

# Pull the Chauvet's "v3" patch series from lore.
b4 am 20251222-vkms-all-config-v3-0-ba42dc3fb9ff@bootlin.com -o ../patches.mbx

# Apply. Patch 19 conflicts on v7.0; skip it (it doesn't affect EDID).
git am ../patches.mbx     # stops on patch 19
git am --skip
git am --continue 

# Stage the patched vkms sources into an out-of-tree package directory.
mkdir -p ../vkms-edid-dkms
cp drivers/gpu/drm/vkms/*.{c,h} ../vkms-edid-dkms/
cd ../vkms-edid-dkms

# Copy the OOT companion files
cp /path/to/fauxput/docs/src/snippets/vkms_oot_shim.h .
cp /path/to/fauxput/docs/src/snippets/Makefile .
cp /path/to/fauxput/docs/src/snippets/dkms.conf .

# Wire the shim into the patched vkms source that uses the stubbed helpers.
sed -i '1i #include "vkms_oot_shim.h"' vkms_config.c

# Build against the running kernel.
make KDIR=/lib/modules/$(uname -r)/build
# produces vkms.ko. Verified against 7.0.3-arch1-2.
```

The companion files:

**`vkms_oot_shim.h`** — stubs the DRM-core helpers that aren't linkable OOT:

```c
{{#include snippets/vkms_oot_shim.h}}
```

**`Makefile`** — standard kernel out-of-tree boilerplate. The `vkms-y` `.o` list mirrors `drivers/gpu/drm/vkms/Makefile` from the patched tree.

```makefile
{{#include snippets/Makefile}}
```

**`dkms.conf`** — DKMS manifest, only needed for Approach B (DKMS-managed install) below. `DEST_MODULE_LOCATION="/updates"` puts the built module ahead of the in-tree `vkms.ko` in modprobe's search order, so loading "vkms" picks up our patched build automatically.

```text
{{#include snippets/dkms.conf}}
```

All three companion files should be reusable across kernel versions, to an extent. You only re-run the b4/git-am dance when a new patch revision lands.

## Loading the module

### Approach A — `insmod` (temporary)

Load the freshly-built `vkms.ko` directly.

```bash
just vkms-insmod
# or, equivalently:
make -C path/to/vkms-edid-dkms -s
sudo modprobe -r vkms
sudo insmod path/to/vkms-edid-dkms/vkms.ko create_default_dev=0
```

If `modprobe -r vkms` fails ("module is in use"), the host compositor is holding it. Reboot or logout/login first.

### Approach B — DKMS (persistent)

Stage under `/usr/src/`, register with DKMS, and DKMS should rebuild the module on every kernel upgrade.

```bash
just vkms-dkms
# or, equivalently:
sudo cp -rT path/to/vkms-edid-dkms /usr/src/vkms-edid-0.1
sudo dkms add -m vkms-edid -v 0.1
sudo dkms install -m vkms-edid -v 0.1
sudo modprobe -r vkms && sudo modprobe vkms
```

To uninstall:

```bash
sudo dkms remove vkms-edid/0.1 --all
sudo rm -rf /usr/src/vkms-edid-0.1
sudo modprobe -r vkms && sudo modprobe vkms   # back to in-tree
```

## Verifying the patched module is active

```bash
sudo mkdir -p /sys/kernel/config/vkms/probe/connectors/0
ls /sys/kernel/config/vkms/probe/connectors/0/
sudo rmdir /sys/kernel/config/vkms/probe/connectors/0 /sys/kernel/config/vkms/probe
```

Listing should include both `edid` and `edid_enabled` (along with `possible_encoders`, `status`, `type`). With in-tree vkms only the latter three appear. `fauxput up` will also stop printing the "EDID write skipped" warning.

## When upstream lands

Watch dri-devel for the v3 / v4 series merging into mainline:

- <https://lore.kernel.org/dri-devel/?q=s%3A%22vkms%3A+Introduce+multiple+configFS%22>
- `git log --grep configfs drivers/gpu/drm/vkms/` in mainline / linux-next

Once the in-tree vkms exposes `edid` and `edid_enabled`:

1. Uninstall the DKMS package (Approach B uninstall above).
2. Reboot or `modprobe -r vkms; modprobe vkms`.
3. fauxput's runtime feature-detection picks up the in-tree attribute automatically — no fauxput code change.

---

## About the patch series

Louis Chauvet's [VKMS configfs attributes patch series](https://lore.kernel.org/dri-devel/?q=s%3A%22vkms%3A+Introduce+multiple+configFS%22) (33 patches, posted to dri-devel December 2025) adds a configfs interface to the kernel's virtual KMS driver, letting userspace configure simulated displays at runtime with writes under `/sys/kernel/config/vkms/`. fauxput only needs the per-connector `edid` and `edid_enabled` attributes from that series (patches 27 and 28), but they build on shared infrastructure earlier in the series, so the patched module ships the full set.

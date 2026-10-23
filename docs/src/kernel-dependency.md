# Kernel dependency: vkms-edid-dkms

fauxput writes per-display EDID into `/sys/kernel/config/vkms/<inst>/connectors/0/edid`. That attribute doesn't exist in mainline Linux 7.0.x, 7.1, or 7.2; it's part of Louis Chauvet's [VKMS configfs attributes patch series](https://lore.kernel.org/dri-devel/?q=s%3A%22vkms%3A+Introduce+multiple+configFS%22). Until that lands in upstream, fauxput needs a patched `vkms.ko`. `fauxput` will detect the `edid` attribute and warns when it's missing; with the patched module loaded, requested resolutions take effect.

## Building the patched module

Nearly all of the patches modify only files under `drivers/gpu/drm/vkms/`. The other patches add helper functions in DRM core, but those helpers are only consumed by `vkms_config_show()` for debugfs output. An OOT shim header (`vkms_oot_shim.h`) replaces them with static stubs, so the patched module compiles against *installed* kernel headers, avoiding the need for a full kernel rebuild.

Prerequisites: `b4`, `git`, `just`, kernel headers for the running kernel (`linux-headers-$(uname -r)` on Debian/Ubuntu, `linux-headers` on Arch), `make`, `patch`, a C compiler.

### Linux 7.0

```bash
# Fetch a clean kernel
git clone --depth 1 --branch v7.0 \
    https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git \
    linux-vkms
cd linux-vkms

# Pull and apply the Chauvet's "v3" patch series from lore.
b4 am 20251222-vkms-all-config-v3-0-ba42dc3fb9ff@bootlin.com
git am ./v3_20251222_louis_chauvet_vkms_introduce_multiple_configfs_attributes.mbx

# Stage the patched vkms sources into an out-of-tree package directory.
mkdir -p ../vkms-edid-dkms
cp drivers/gpu/drm/vkms/*.{c,h} ../vkms-edid-dkms/
cd ../vkms-edid-dkms

# Copy the OOT companion files
cp /path/to/fauxput/docs/src/snippets/vkms_oot_shim.h .
cp /path/to/fauxput/docs/src/snippets/Makefile .
cp /path/to/fauxput/docs/src/snippets/dkms.conf .

# Wire the shim into the patched vkms source that uses the stubbed helpers.
sed -i '/#include "vkms_config.h"/i #include "vkms_oot_shim.h"' vkms_config.c

# Build against the running kernel.
make KDIR=/lib/modules/$(uname -r)/build
# produces vkms.ko
```

### Linux 7.1 and 7.2

Use Chauvet's v5 series. The helpers below apply the OOT shim and, for 7.1, a compatibility patch for `drm_atomic_state`.

Run from the fauxput checkout, using new directories:

```bash
# Fetch the v5 base.
git init linux-vkms-v5
git -C linux-vkms-v5 fetch --depth=1 \
    https://gitlab.freedesktop.org/drm/misc/kernel.git \
    6648301c5bb2ef23f0fb15bcb01d21ff66f36799
git -C linux-vkms-v5 checkout --detach FETCH_HEAD

# Pull and apply all 38 v5 patches from lore.
mkdir vkms-v5-mail
b4 am -v 5 -n vkms-v5 -o vkms-v5-mail \
    20260627-vkms-all-config-v5-0-854aa0840926@bootlin.com
git -C linux-vkms-v5 am ../vkms-v5-mail/vkms-v5.mbx
```

Choose your kernel below. `just vkms-prepare` copies and patches the sources into a new output directory.

#### Linux 7.1

```bash
just vkms-prepare 7.1 linux-vkms-v5 vkms-edid-7.1
make -C vkms-edid-7.1 KDIR=/lib/modules/$(uname -r)/build
# produces vkms.ko
```

#### Linux 7.2

```bash
just vkms-prepare 7.2 linux-vkms-v5 vkms-edid-7.2
make -C vkms-edid-7.2 KDIR=/lib/modules/$(uname -r)/build
# produces vkms.ko
```

To build for another installed kernel, set `KDIR=/lib/modules/<kernel-release>/build`.

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

For 7.1/7.2, `just vkms-prepare` adjusts the package version and adds a kernel-version restriction.

## Loading the module

### Approach A — `insmod` (temporary)

Load the freshly-built `vkms.ko` directly.

```bash
VKMS_SRC=path/to/vkms-edid-dkms just vkms-insmod
# or, equivalently:
make -C path/to/vkms-edid-dkms -s
sudo modprobe -r vkms
sudo insmod path/to/vkms-edid-dkms/vkms.ko create_default_dev=0
```

If `modprobe -r vkms` fails ("module is in use"), the host compositor is holding it. Reboot or logout/login first. Do not unload VKMS from an active streaming session.

### Approach B — DKMS (persistent)

Stage under `/usr/src/`, register with DKMS, and DKMS should rebuild the module on every kernel upgrade within the supported series.

Set `VKMS_SRC` to your prepared source directory, such as `vkms-edid-7.1` or `vkms-edid-7.2`. `just vkms-dkms` reads the version from `dkms.conf`: `0.1` for 7.0, `0.3-7.1` for 7.1, or `0.3-7.2` for 7.2. Adjust the version in the manual commands accordingly.

```bash
VKMS_SRC=path/to/vkms-edid-dkms just vkms-dkms
# or, equivalently:
sudo cp -rT path/to/vkms-edid-dkms /usr/src/vkms-edid-0.1
sudo dkms add -m vkms-edid -v 0.1
sudo dkms install -m vkms-edid -v 0.1
```

Reboot to load the installed module; the helper does not reload VKMS.

To uninstall:

```bash
sudo dkms remove vkms-edid/0.1 --all
sudo rm -rf /usr/src/vkms-edid-0.1
# Reboot to use the in-tree module.
```

### Updating an existing installation

Keep your old DKMS sources and a working kernel until you've tested the new module. The helper won't overwrite an existing `/usr/src/vkms-edid-*` directory.

If you keep both 7.1 and 7.2 kernels, install the matching DKMS version explicitly after updates; autoinstall doesn't select between patchsets.

## Verifying the patched module is active

```bash
sudo mkdir -p /sys/kernel/config/vkms/probe/connectors/0
ls /sys/kernel/config/vkms/probe/connectors/0/
sudo rmdir /sys/kernel/config/vkms/probe/connectors/0 /sys/kernel/config/vkms/probe
```

Listing should include both `edid` and `edid_enabled` (along with `possible_encoders`, `status`, `type`). With in-tree vkms only the latter three appear. `fauxput up` will also stop printing the "EDID write skipped" warning.


## About the patch series

Louis Chauvet's [VKMS configfs attributes patch series](https://lore.kernel.org/dri-devel/?q=s%3A%22vkms%3A+Introduce+multiple+configFS%22) (v3: 33 patches, December 2025; v5: 38 patches, June 2026) adds a configfs interface to the kernel's virtual KMS driver, letting userspace configure simulated displays at runtime with writes under `/sys/kernel/config/vkms/`. fauxput only needs the per-connector `edid` and `edid_enabled` attributes from that series (patches 27 and 28 in v3, 32 and 33 in v5), but they build on shared infrastructure earlier in the series, so I suggest applying the full patchset.

## When upstream lands

Watch dri-devel for the configfs series merging into mainline:

- <https://lore.kernel.org/dri-devel/?q=s%3A%22vkms%3A+Introduce+multiple+configFS%22>
- `git log --grep configfs drivers/gpu/drm/vkms/` in mainline/linux-next

Once the in-tree vkms exposes `edid` and `edid_enabled`:

1. Uninstall the DKMS package (Approach B uninstall above).
2. Reboot, or reload VKMS when no compositor is using it.

# Out-of-tree build of patched vkms (with v3 EDID-via-configfs support).
# Targets the running kernel by default; override KDIR for cross-builds.

KDIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

obj-m := vkms.o

# Mirror drivers/gpu/drm/vkms/Makefile object list. Tests are skipped.
vkms-y := \
	vkms_drv.o \
	vkms_plane.o \
	vkms_output.o \
	vkms_crtc.o \
	vkms_composer.o \
	vkms_writeback.o \
	vkms_formats.o \
	vkms_luts.o \
	vkms_colorop.o \
	vkms_connector.o \
	vkms_config.o \
	vkms_configfs.o

# vkms_config_show / vkms_configfs touch sysfs/configfs heavily; ensure
# CONFIG_DRM_VKMS_CONFIGFS is enabled in the running kernel's .config.

all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean

install:
	$(MAKE) -C $(KDIR) M=$(PWD) modules_install

.PHONY: all clean install

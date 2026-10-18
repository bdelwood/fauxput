# Out-of-tree build of VKMS with Chauvet's v3 or v5 configfs series.
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

# Requires the running kernel's DRM, configfs, and module build support.

all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean

install:
	$(MAKE) -C $(KDIR) M=$(PWD) modules_install

.PHONY: all clean install

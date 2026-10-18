/* SPDX-License-Identifier: GPL-2.0+ */
#ifndef VKMS_OOT_SHIM_H
#define VKMS_OOT_SHIM_H

/* Local debugfs helpers; do not change DRM core or functional color handling. */
#include <drm/drm_color_mgmt.h>
#include <drm/drm_connector.h>
#include <drm/drm_plane.h>

#define drm_get_color_encoding_name vkms_oot_color_encoding_name
#define drm_get_color_range_name vkms_oot_color_range_name
#define drm_get_colorspace_name vkms_oot_colorspace_name
#define drm_get_plane_type_name vkms_oot_plane_type_name
#define drm_get_rotation_name vkms_oot_rotation_name

static inline const char *vkms_oot_color_encoding_name(unsigned int encoding)
{
	switch (encoding) {
	case DRM_COLOR_YCBCR_BT601: return "ITU-R BT.601 YCbCr";
	case DRM_COLOR_YCBCR_BT709: return "ITU-R BT.709 YCbCr";
	case DRM_COLOR_YCBCR_BT2020: return "ITU-R BT.2020 YCbCr";
	default: return "unknown";
	}
}

static inline const char *vkms_oot_color_range_name(unsigned int range)
{
	switch (range) {
	case DRM_COLOR_YCBCR_LIMITED_RANGE: return "YCbCr limited range";
	case DRM_COLOR_YCBCR_FULL_RANGE: return "YCbCr full range";
	default: return "unknown";
	}
}

static inline const char *vkms_oot_colorspace_name(unsigned int colorspace)
{
	switch (colorspace) {
	case DRM_MODE_COLORIMETRY_DEFAULT: return "Default";
	case DRM_MODE_COLORIMETRY_BT2020_RGB: return "BT2020_RGB";
	default: return "other colorspace";
	}
}

static inline const char *vkms_oot_plane_type_name(unsigned int type)
{
	switch (type) {
	case DRM_PLANE_TYPE_PRIMARY: return "Primary";
	case DRM_PLANE_TYPE_CURSOR: return "Cursor";
	case DRM_PLANE_TYPE_OVERLAY: return "Overlay";
	default: return "Unknown";
	}
}

static inline const char *vkms_oot_rotation_name(unsigned int rotation)
{
	switch (rotation) {
	case 0: return "rotate-0";
	case 1: return "rotate-90";
	case 2: return "rotate-180";
	case 3: return "rotate-270";
	case 4: return "reflect-x";
	case 5: return "reflect-y";
	default: return "unknown";
	}
}

#ifndef DRM_ROTATION_FMT
#define DRM_ROTATION_FMT "%s%s%s%s%s%s"
#define DRM_ROTATION_FMT_ARGS(rot) \
	((rot) & DRM_MODE_ROTATE_0 ? "rotate-0 " : ""), \
	((rot) & DRM_MODE_ROTATE_90 ? "rotate-90 " : ""), \
	((rot) & DRM_MODE_ROTATE_180 ? "rotate-180 " : ""), \
	((rot) & DRM_MODE_ROTATE_270 ? "rotate-270 " : ""), \
	((rot) & DRM_MODE_REFLECT_X ? "reflect-x " : ""), \
	((rot) & DRM_MODE_REFLECT_Y ? "reflect-y " : "")
#endif

#endif

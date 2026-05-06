#ifndef _VKMS_OOT_SHIM_H_
#define _VKMS_OOT_SHIM_H_
/* Stubs for DRM-core helpers not linkable from an OOT module on
 * Linux 7.0.x. These are debugfs-only paths consumed by
 * vkms_config_show(); functional vkms code is unaffected. */
static inline const char *drm_get_color_encoding_name(int v)  { return "?"; }
static inline const char *drm_get_color_range_name(int v)     { return "?"; }
static inline const char *drm_get_colorspace_name(int v)      { return "?"; }
static inline const char *drm_get_plane_type_name(int v)      { return "?"; }
static inline const char *drm_get_rotation_name(unsigned v)   { return "?"; }
#endif

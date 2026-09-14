# Interior contours, ordered dithering and sensor resolution

The raster mesh shader retained six hard steps for sun diffuse and hemisphere illumination. Removed those upstream steps so smooth normals and lighting remain continuous until the display pass. Source mesh facets and textures remain present.

The shader backend previously forced retro=0 and dither=0, applying only one linear-RGBA8 rounding step of Bayer coverage. It now uses 31 display intervals (32 tones per channel), full-step fixed 4x4 ordered coverage, and the existing distance attenuation. Native linear-LDR rounding coverage also runs after display-palette conversion. Exposure is unchanged; no temporal or random noise is introduced.

The shader launcher now defaults to 576x324 instead of 640x360: 10% fewer pixels on each axis, 19% fewer total. KESSEL_INTERNAL_RES can explicitly override it. Added 576x324 to the renderer supported sizes. The mechanic capture tool now supports this size and consistently requests a 1920x1080 presentation instead of assuming a threefold native upscale. Its manifests report the actual native sensor dimensions.

Validation: release build succeeded; 7 shader/ABI tests and 3 display-quantization tests passed. GPU captures loader_dither_20260914 and optics_dither_20260914 both report native_resolution=[576,324]; all 4 loader and 6 optics outcome checks passed. Loader cabin and focused periscope frames were visually inspected. See captures/astra/loader_dither_20260914.png and captures/astra/optics_dither_focus_20260914.png. Build/test logs are in artifacts/visual-feedback-20260914/.

Changes are in the shader feature worktree. Relaunch using tools/launch_shader_amoled.sh after closing the current game to use this build and resolution.

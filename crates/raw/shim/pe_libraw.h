/*
 * Minimal, stable C ABI over LibRaw for the Rust decoder.
 *
 * Rust never touches libraw_data_t directly (its layout changes between LibRaw
 * versions); this shim owns all struct access and exposes plain types only.
 */
#ifndef PE_LIBRAW_H
#define PE_LIBRAW_H

#include <stddef.h>
#include <stdint.h>

typedef int (*pe_cancel_fn)(void *ctx);

typedef struct pe_raw_info {
    /* Dimensions of the decoded output (after orientation and any downscale). */
    uint32_t width;
    uint32_t height;
    /* Output dimensions at full scale (after orientation). */
    uint32_t full_width;
    uint32_t full_height;
    int32_t flip;
    int32_t half_size;
    float iso;
    float shutter;
    float aperture;
    float focal_length;
    char make[64];
    char model[64];
} pe_raw_info;

typedef struct pe_raw_ctx pe_raw_ctx;

/*
 * Decodes `path` into 16-bit linear RGB (sRGB primaries, camera white balance).
 * If `min_long_edge` > 0 and a half-size decode still has a long edge >= it, LibRaw's
 * fast half-size mode is used. `cancel` is polled at LibRaw progress stages.
 * If `max_threads` > 0 and LibRaw was built with OpenMP, its parallel regions started
 * from the calling thread use at most that many threads.
 * Returns 0 on success (then *out_ctx must be released) or a LibRaw error code.
 */
int pe_raw_decode(const char *path, uint32_t min_long_edge, uint32_t max_threads, pe_cancel_fn cancel,
                  void *cancel_ctx, pe_raw_ctx **out_ctx, pe_raw_info *info);

/* Copies width*height*3 samples into dst. Returns 0 on success. */
int pe_raw_copy_rgb16(const pe_raw_ctx *ctx, uint16_t *dst, size_t samples);

void pe_raw_release(pe_raw_ctx *ctx);

/* ---- Embedded preview ---- */

typedef struct pe_thumb_info {
    int32_t format;   /* 1 = JPEG bytes, 2 = 8-bit RGB bitmap */
    uint32_t width;   /* for bitmaps; JPEG dimensions come from the JPEG itself */
    uint32_t height;
    int32_t flip;     /* orientation of the main image (dcraw flip semantics) */
    uint32_t available; /* number of embedded previews in the file */
} pe_thumb_info;

typedef struct pe_raw_thumb pe_raw_thumb;

/*
 * Extracts an embedded preview without decoding the raw data: the smallest one whose
 * long edge is >= min_long_edge (or the largest if none is). Returns 0 on success
 * (then *out must be released), LIBRAW_NO_THUMBNAIL if the file has none, or a LibRaw
 * error code. `data` and `len` point into memory owned by `out`.
 */
int pe_raw_thumbnail(const char *path, uint32_t min_long_edge, pe_raw_thumb **out, pe_thumb_info *info,
                     const uint8_t **data, size_t *len);
void pe_raw_thumb_release(pe_raw_thumb *thumb);

/* ---- Metadata (headers only, no decode) ---- */

typedef struct pe_raw_meta {
    char make[64];
    char model[64];
    char lens[128];
    /* Capture time as the camera recorded it (local wall-clock, no zone):
     * "YYYY-MM-DDTHH:MM:SS", or empty if unknown. */
    char captured_at[20];
    float iso;
    float aperture;
    float shutter;
    float focal_length;
    /* Output dimensions at full scale, after orientation. */
    uint32_t width;
    uint32_t height;
    int32_t flip;
    int32_t has_gps;
    double latitude;
    double longitude;
} pe_raw_meta;

/* Parses `path`'s headers (no raw data unpacking). Returns 0 or a LibRaw error. */
int pe_raw_metadata(const char *path, pe_raw_meta *out);

const char *pe_raw_strerror(int code);
const char *pe_raw_libraw_version(void);

#endif

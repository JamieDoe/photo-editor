#include "pe_libraw.h"

#include <libraw/libraw.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifndef _WIN32
#include <dlfcn.h>
#endif

struct pe_raw_ctx {
    libraw_data_t *lr;
    libraw_processed_image_t *img;
    pe_cancel_fn cancel;
    void *cancel_ctx;
};

static int progress_cb(void *data, enum LibRaw_progress stage, int iteration, int expected) {
    (void)stage;
    (void)iteration;
    (void)expected;
    pe_raw_ctx *ctx = (pe_raw_ctx *)data;
    /* Non-zero return makes LibRaw abort with LIBRAW_CANCELLED_BY_CALLBACK. */
    return (ctx->cancel && ctx->cancel(ctx->cancel_ctx)) ? 1 : 0;
}

/* Caps OpenMP threads for regions started from this thread. Looked up at runtime so
 * the shim neither links nor requires an OpenMP runtime: if LibRaw was built without
 * OpenMP the symbol is absent and LibRaw is single-threaded anyway. */
static void limit_openmp_threads(uint32_t max_threads) {
#ifndef _WIN32
    if (max_threads == 0) return;
    typedef void (*set_fn)(int);
    set_fn set = (set_fn)dlsym(RTLD_DEFAULT, "omp_set_num_threads");
    if (set) set((int)max_threads);
#else
    (void)max_threads; /* TODO(phase1): GetProcAddress on the bundled LibRaw's OpenMP runtime. */
#endif
}

static void copy_str(char *dst, const char *src, size_t n) {
    strncpy(dst, src, n - 1);
    dst[n - 1] = '\0';
}

int pe_raw_decode(const char *path, uint32_t min_long_edge, uint32_t max_threads, pe_cancel_fn cancel,
                  void *cancel_ctx, pe_raw_ctx **out_ctx, pe_raw_info *info) {
    *out_ctx = NULL;
    memset(info, 0, sizeof(*info));

    pe_raw_ctx *ctx = (pe_raw_ctx *)calloc(1, sizeof(pe_raw_ctx));
    if (!ctx) return LIBRAW_UNSUFFICIENT_MEMORY;
    ctx->cancel = cancel;
    ctx->cancel_ctx = cancel_ctx;

    ctx->lr = libraw_init(0);
    if (!ctx->lr) {
        free(ctx);
        return LIBRAW_UNSUFFICIENT_MEMORY;
    }
    libraw_data_t *lr = ctx->lr;
    libraw_set_progress_handler(lr, progress_cb, ctx);
    limit_openmp_threads(max_threads);

    /* Scene-linear 16-bit output in sRGB primaries with as-shot white balance.
     * gamma (1,1) + no_auto_bright keeps values proportional to sensor data, with the
     * sensor clip point at 65535. Tone and display encoding happen in our renderer. */
    lr->params.output_bps = 16;
    lr->params.gamm[0] = 1.0;
    lr->params.gamm[1] = 1.0;
    lr->params.no_auto_bright = 1;
    lr->params.use_camera_wb = 1;
    lr->params.output_color = 1; /* sRGB primaries */
    lr->params.highlight = 0;    /* clip */

    int rc = libraw_open_file(lr, path);
    if (rc != LIBRAW_SUCCESS) goto fail;

    {
        uint32_t w = lr->sizes.width, h = lr->sizes.height;
        if (lr->sizes.flip & 4) { uint32_t t = w; w = h; h = t; }
        info->full_width = w;
        info->full_height = h;
        uint32_t long_edge = w > h ? w : h;
        lr->params.half_size = (min_long_edge > 0 && long_edge / 2 >= min_long_edge) ? 1 : 0;
    }

    rc = libraw_unpack(lr);
    if (rc != LIBRAW_SUCCESS) goto fail;
    rc = libraw_dcraw_process(lr);
    if (rc != LIBRAW_SUCCESS) goto fail;

    ctx->img = libraw_dcraw_make_mem_image(lr, &rc);
    if (!ctx->img) goto fail;
    if (ctx->img->type != LIBRAW_IMAGE_BITMAP || ctx->img->colors != 3 || ctx->img->bits != 16) {
        rc = LIBRAW_DATA_ERROR;
        goto fail;
    }

    info->width = ctx->img->width;
    info->height = ctx->img->height;
    info->flip = lr->sizes.flip;
    info->half_size = lr->params.half_size;
    info->iso = lr->other.iso_speed;
    info->shutter = lr->other.shutter;
    info->aperture = lr->other.aperture;
    info->focal_length = lr->other.focal_len;
    copy_str(info->make, lr->idata.make, sizeof(info->make));
    copy_str(info->model, lr->idata.model, sizeof(info->model));

    /* Free LibRaw's working buffers now; only the processed image is still needed.
     * This lowers peak memory while Rust copies the result. */
    libraw_recycle(lr);

    *out_ctx = ctx;
    return LIBRAW_SUCCESS;

fail:
    pe_raw_release(ctx);
    return rc != LIBRAW_SUCCESS ? rc : LIBRAW_UNSPECIFIED_ERROR;
}

int pe_raw_copy_rgb16(const pe_raw_ctx *ctx, uint16_t *dst, size_t samples) {
    if (!ctx || !ctx->img) return LIBRAW_UNSPECIFIED_ERROR;
    size_t expected = (size_t)ctx->img->width * ctx->img->height * 3;
    if (samples != expected || ctx->img->data_size != expected * sizeof(uint16_t))
        return LIBRAW_DATA_ERROR;
    memcpy(dst, ctx->img->data, expected * sizeof(uint16_t));
    return LIBRAW_SUCCESS;
}

void pe_raw_release(pe_raw_ctx *ctx) {
    if (!ctx) return;
    if (ctx->img) libraw_dcraw_clear_mem(ctx->img);
    if (ctx->lr) libraw_close(ctx->lr);
    free(ctx);
}

struct pe_raw_thumb {
    libraw_data_t *lr;
    libraw_processed_image_t *img;
};

void pe_raw_thumb_release(pe_raw_thumb *t) {
    if (!t) return;
    if (t->img) libraw_dcraw_clear_mem(t->img);
    if (t->lr) libraw_close(t->lr);
    free(t);
}

/* Index of the preview to extract, or -1 to let LibRaw pick its default. */
static int choose_thumb(const libraw_thumbnail_list_t *list, uint32_t min_long_edge) {
    int best = -1, largest = -1;
    uint32_t best_edge = UINT32_MAX, largest_edge = 0;
    for (int i = 0; i < list->thumbcount && i < LIBRAW_THUMBNAIL_MAXCOUNT; i++) {
        const libraw_thumbnail_item_t *it = &list->thumblist[i];
        if (it->tformat != LIBRAW_INTERNAL_THUMBNAIL_JPEG) continue; /* JPEG is universal */
        uint32_t edge = it->twidth > it->theight ? it->twidth : it->theight;
        if (edge >= min_long_edge && edge < best_edge) { best = i; best_edge = edge; }
        if (edge > largest_edge) { largest = i; largest_edge = edge; }
    }
    return best >= 0 ? best : largest;
}

int pe_raw_thumbnail(const char *path, uint32_t min_long_edge, pe_raw_thumb **out, pe_thumb_info *info,
                     const uint8_t **data, size_t *len) {
    *out = NULL;
    *data = NULL;
    *len = 0;
    memset(info, 0, sizeof(*info));
    pe_raw_thumb *t = (pe_raw_thumb *)calloc(1, sizeof(pe_raw_thumb));
    if (!t) return LIBRAW_UNSUFFICIENT_MEMORY;
    t->lr = libraw_init(0);
    if (!t->lr) {
        free(t);
        return LIBRAW_UNSUFFICIENT_MEMORY;
    }
    int rc = libraw_open_file(t->lr, path);
    if (rc != LIBRAW_SUCCESS) goto fail;

    info->available = (uint32_t)t->lr->thumbs_list.thumbcount;
    info->flip = t->lr->sizes.flip;
    int idx = choose_thumb(&t->lr->thumbs_list, min_long_edge);
    rc = idx >= 0 ? libraw_unpack_thumb_ex(t->lr, idx) : libraw_unpack_thumb(t->lr);
    if (rc != LIBRAW_SUCCESS) goto fail;
    t->img = libraw_dcraw_make_mem_thumb(t->lr, &rc);
    if (!t->img) goto fail;

    if (t->img->type == LIBRAW_IMAGE_JPEG) {
        info->format = 1;
    } else if (t->img->type == LIBRAW_IMAGE_BITMAP && t->img->colors == 3 && t->img->bits == 8) {
        info->format = 2;
        info->width = t->img->width;
        info->height = t->img->height;
    } else {
        rc = LIBRAW_NO_THUMBNAIL; /* formats we don't handle (16-bit, layered, H.265) */
        goto fail;
    }
    *data = t->img->data;
    *len = t->img->data_size;
    *out = t;
    return LIBRAW_SUCCESS;

fail:
    pe_raw_thumb_release(t);
    return rc != LIBRAW_SUCCESS ? rc : LIBRAW_UNSPECIFIED_ERROR;
}

static double gps_degrees(const float dms[3], char ref) {
    double d = dms[0] + dms[1] / 60.0 + dms[2] / 3600.0;
    return (ref == 'S' || ref == 'W') ? -d : d;
}

int pe_raw_metadata(const char *path, pe_raw_meta *out) {
    memset(out, 0, sizeof(*out));
    libraw_data_t *lr = libraw_init(0);
    if (!lr) return LIBRAW_UNSUFFICIENT_MEMORY;
    int rc = libraw_open_file(lr, path); /* parses headers only */
    if (rc != LIBRAW_SUCCESS) {
        libraw_close(lr);
        return rc;
    }
    copy_str(out->make, lr->idata.make, sizeof(out->make));
    copy_str(out->model, lr->idata.model, sizeof(out->model));
    const char *lens = lr->lens.Lens[0] ? lr->lens.Lens : lr->lens.makernotes.Lens;
    copy_str(out->lens, lens, sizeof(out->lens));
    if (lr->other.timestamp > 0) {
        /* LibRaw built the timestamp from the camera's local fields with mktime();
         * localtime() on this machine recovers those fields exactly. */
        time_t ts = lr->other.timestamp;
        struct tm tm_buf;
        if (localtime_r(&ts, &tm_buf)) {
            strftime(out->captured_at, sizeof(out->captured_at), "%Y-%m-%dT%H:%M:%S", &tm_buf);
        }
    }
    out->iso = lr->other.iso_speed;
    out->aperture = lr->other.aperture;
    out->shutter = lr->other.shutter;
    out->focal_length = lr->other.focal_len;
    uint32_t w = lr->sizes.width, h = lr->sizes.height;
    if (lr->sizes.flip & 4) { uint32_t t = w; w = h; h = t; }
    out->width = w;
    out->height = h;
    out->flip = lr->sizes.flip;
    if (lr->other.parsed_gps.gpsparsed) {
        out->has_gps = 1;
        out->latitude = gps_degrees(lr->other.parsed_gps.latitude, lr->other.parsed_gps.latref);
        out->longitude = gps_degrees(lr->other.parsed_gps.longitude, lr->other.parsed_gps.longref);
    }
    libraw_close(lr);
    return LIBRAW_SUCCESS;
}

const char *pe_raw_strerror(int code) { return libraw_strerror(code); }

const char *pe_raw_libraw_version(void) { return libraw_version(); }

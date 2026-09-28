/*
 * Minimal entry points over libjpeg-turbo's TurboJPEG 3 API. Kept in C so parameter,
 * pixel-format and scaling constants come from the real header.
 */
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <turbojpeg.h>

static void set_err(char *err, size_t err_len, const char *msg) {
    if (!err || err_len == 0) return;
    strncpy(err, msg, err_len - 1);
    err[err_len - 1] = '\0';
}

/* ---- Encode ---- */

/* Encodes interleaved 8-bit RGB (channels = 3) or RGBX (channels = 4, 4th ignored)
 * with 4:4:4 chroma. On success returns 0 and *out must be freed with pe_tj_free. */
int pe_tj_encode(const uint8_t *pixels, int width, int height, int channels, int quality,
                 uint8_t **out, size_t *out_len, char *err, size_t err_len) {
    *out = NULL;
    *out_len = 0;
    int format = channels == 4 ? TJPF_RGBX : TJPF_RGB;
    tjhandle tj = tj3Init(TJINIT_COMPRESS);
    if (!tj) {
        set_err(err, err_len, "tj3Init failed");
        return -1;
    }
    int rc = 0;
    if (tj3Set(tj, TJPARAM_QUALITY, quality) || tj3Set(tj, TJPARAM_SUBSAMP, TJSAMP_444) ||
        tj3Compress8(tj, pixels, width, 0, height, format, out, out_len)) {
        set_err(err, err_len, tj3GetErrorStr(tj));
        rc = -1;
    }
    tj3Destroy(tj);
    return rc;
}

void pe_tj_free(uint8_t *buf) { tj3Free(buf); }

/* ---- Scaled decode ---- */

/* Picks the smallest power-of-two scale (1, 1/2, 1/4, 1/8) whose scaled long edge is
 * still >= min_long_edge; 1/1 if even full size is smaller. Power-of-two only, so the
 * result matches repeated 2x downsampling (the pyramid's semantics) and never
 * undershoots what a halving loop would keep. Deterministic, so the size query and
 * the decode agree. */
static tjscalingfactor choose_scale(int width, int height, uint32_t min_long_edge) {
    tjscalingfactor best = {1, 1};
    int n = 0;
    tjscalingfactor *factors = tj3GetScalingFactors(&n);
    int long_edge = width > height ? width : height;
    int best_edge = long_edge;
    for (int i = 0; factors && i < n; i++) {
        tjscalingfactor f = factors[i];
        if (f.num != 1 || (f.denom & (f.denom - 1)) != 0) continue; /* 1/2^k only */
        int edge = TJSCALED(long_edge, f);
        if ((uint32_t)edge >= min_long_edge && edge < best_edge) {
            best = f;
            best_edge = edge;
        }
    }
    return best;
}

/* Reads the header and reports source and scaled dimensions. Returns 0 on success. */
int pe_tj_scaled_size(const uint8_t *jpeg, size_t len, uint32_t min_long_edge, uint32_t *src_w,
                      uint32_t *src_h, uint32_t *out_w, uint32_t *out_h, char *err, size_t err_len) {
    tjhandle tj = tj3Init(TJINIT_DECOMPRESS);
    if (!tj) {
        set_err(err, err_len, "tj3Init failed");
        return -1;
    }
    int rc = 0;
    if (tj3DecompressHeader(tj, jpeg, len)) {
        set_err(err, err_len, tj3GetErrorStr(tj));
        rc = -1;
    } else {
        int w = tj3Get(tj, TJPARAM_JPEGWIDTH), h = tj3Get(tj, TJPARAM_JPEGHEIGHT);
        if (w <= 0 || h <= 0) {
            set_err(err, err_len, "invalid JPEG dimensions");
            rc = -1;
        } else {
            tjscalingfactor f = choose_scale(w, h, min_long_edge);
            *src_w = (uint32_t)w;
            *src_h = (uint32_t)h;
            *out_w = (uint32_t)TJSCALED(w, f);
            *out_h = (uint32_t)TJSCALED(h, f);
        }
    }
    tj3Destroy(tj);
    return rc;
}

/* Decodes to interleaved 8-bit RGB at the scale chosen by pe_tj_scaled_size.
 * dst must hold out_w * out_h * 3 bytes. Returns 0 on success. */
int pe_tj_decode_scaled(const uint8_t *jpeg, size_t len, uint32_t min_long_edge, uint8_t *dst,
                        size_t dst_len, char *err, size_t err_len) {
    tjhandle tj = tj3Init(TJINIT_DECOMPRESS);
    if (!tj) {
        set_err(err, err_len, "tj3Init failed");
        return -1;
    }
    int rc = -1;
    if (tj3DecompressHeader(tj, jpeg, len)) {
        set_err(err, err_len, tj3GetErrorStr(tj));
        goto done;
    }
    int w = tj3Get(tj, TJPARAM_JPEGWIDTH), h = tj3Get(tj, TJPARAM_JPEGHEIGHT);
    tjscalingfactor f = choose_scale(w, h, min_long_edge);
    size_t need = (size_t)TJSCALED(w, f) * (size_t)TJSCALED(h, f) * 3;
    if (dst_len < need) {
        set_err(err, err_len, "destination buffer too small");
        goto done;
    }
    if (tj3SetScalingFactor(tj, f) || tj3Decompress8(tj, jpeg, len, dst, 0, TJPF_RGB)) {
        set_err(err, err_len, tj3GetErrorStr(tj));
        goto done;
    }
    rc = 0;
done:
    tj3Destroy(tj);
    return rc;
}

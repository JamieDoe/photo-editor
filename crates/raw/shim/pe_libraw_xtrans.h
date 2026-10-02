/*
 * Internal to the shim: a LibRaw instance whose X-Trans processing is deterministic
 * under OpenMP. See pe_libraw_xtrans.cpp and docs/ADR/0061-deterministic-xtrans-decode.md.
 */
#ifndef PE_LIBRAW_XTRANS_H
#define PE_LIBRAW_XTRANS_H

#include <libraw/libraw.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Drop-in replacement for libraw_init(0): use with the LibRaw C API and release with
 * libraw_close(). Returns NULL when out of memory. */
libraw_data_t *pe_libraw_new(void);

#ifdef __cplusplus
}
#endif

#endif

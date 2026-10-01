/*
 * Deterministic X-Trans processing on top of LibRaw (ADR 0060).
 *
 * LibRaw 0.22's OpenMP code has two data races that only affect X-Trans sensors, so
 * decoding the same RAF twice gives different pixels:
 *
 *  1. Half-size (preview) decodes: copy_bayer() bins each 2x2 cell into one output
 *     pixel, splitting rows across threads. A Bayer cell holds one sample of each
 *     colour; an X-Trans cell can hold two of the same, which then come from rows 2k
 *     and 2k+1 on different threads. Whichever thread writes last wins.
 *  2. Full-size decodes: xtrans_interpolate() runs 512-row strips in parallel. Each
 *     strip copies 8 context rows that the strip above is still overwriting with its
 *     final output, so seam pixels depend on thread timing.
 *
 * Fixes, using LibRaw's own extension points (no patched LibRaw needed):
 *
 *  1. copy_bayer() is a virtual "hotspot"; for X-Trans with shrink it is replaced by
 *     a parallel copy in which each task owns whole output rows. The result is
 *     exactly LibRaw's single-threaded one.
 *  2. The interpolate_xtrans_cb callback replaces the strip loop. The image is split
 *     into a fixed grid of blocks, one LibRaw tile each, and LibRaw's xtrans_interpolate
 *     demosaics each block single-threaded in a private buffer. Blocks run in
 *     parallel, in four checkerboard phases: a block reads up to its neighbours'
 *     output regions but never a region written in its own phase. Every block's input
 *     is therefore fixed, and the grid does not depend on the thread count, so the
 *     output is identical for any number of threads.
 *
 * OpenMP is resolved with dlsym (as in pe_libraw.c): when LibRaw has no OpenMP it is
 * single-threaded and already deterministic, and LibRaw's own code paths are used.
 */
#include "pe_libraw_xtrans.h"

#include <algorithm>
#include <atomic>
#include <cstring>
#include <exception>
#include <memory>
#include <mutex>
#include <system_error>
#include <thread>
#include <vector>
#ifndef _WIN32
#include <dlfcn.h>
#endif

namespace {

struct OmpApi {
    void (*set_num_threads)(int) = nullptr;
    int (*get_max_threads)() = nullptr;
    bool available() const { return set_num_threads && get_max_threads; }
};

const OmpApi &omp() {
    static const OmpApi api = [] {
        OmpApi a;
#ifndef _WIN32
        a.set_num_threads = reinterpret_cast<void (*)(int)>(dlsym(RTLD_DEFAULT, "omp_set_num_threads"));
        a.get_max_threads = reinterpret_cast<int (*)()>(dlsym(RTLD_DEFAULT, "omp_get_max_threads"));
#endif
        /* TODO(phase1): Windows, alongside limit_openmp_threads in pe_libraw.c. Until
         * then the X-Trans races remain there if the bundled LibRaw uses OpenMP. */
        return a;
    }();
    return api;
}

/* Caps OpenMP regions started from this thread for the scope. */
class OmpThreads {
  public:
    explicit OmpThreads(int n) {
        if (!omp().available()) return;
        prev_ = omp().get_max_threads();
        omp().set_num_threads(n);
    }
    ~OmpThreads() {
        if (prev_ > 0) omp().set_num_threads(prev_);
    }
    OmpThreads(const OmpThreads &) = delete;
    OmpThreads &operator=(const OmpThreads &) = delete;

  private:
    int prev_ = 0;
};

/* xtrans_interpolate rejects images smaller than one tile in either direction. */
constexpr int kTile = LIBRAW_AHD_TILE;
/* Context kept around a block's output: LibRaw's own 8-pixel tile margin plus the 3
 * pixels at the block edge that the tile loop skips, rounded up to the X-Trans period. */
constexpr int kMargin = 12;
/* Output per block, a multiple of 6, sized so the block input (kStep + 2 * kMargin =
 * 516) stays within one LibRaw tile: xtrans_interpolate starts a second tile only
 * beyond kTile + 6 pixels. */
constexpr int kStep = (kTile + 6 - 2 * kMargin) / 6 * 6;
static_assert(kStep % 6 == 0 && kMargin % 6 == 0, "blocks must keep the X-Trans pattern phase");

struct Span {
    int in0, in1;   /* input (context included) */
    int out0, out1; /* output written back */
};

/* Splits [0, len) into blocks of kStep output pixels; the last block also takes the
 * remainder (LibRaw demosaics that as a partial second tile, which is cheaper than a
 * block of its own). Every input start is a multiple of 6, so a block sees the same
 * X-Trans pattern phase as the full image; every input is at least kTile long; and a
 * block's input reaches at most into its immediate neighbours' outputs (a last block
 * pulled back to kTile starts at most 25 pixels before its own output). Requires
 * len >= kTile. */
std::vector<Span> split(int len) {
    const int n = std::max(1, len / kStep);
    std::vector<Span> spans;
    for (int k = 0; k < n; k++) {
        const bool first = k == 0, last = k == n - 1;
        Span s;
        s.out0 = k * kStep;
        s.out1 = last ? len : s.out0 + kStep;
        s.in0 = first ? 0 : s.out0 - kMargin;
        s.in1 = last ? len : s.out1 + kMargin;
        if (s.in1 - s.in0 < kTile) {
            if (last)
                s.in0 = std::max(0, (len - kTile) / 6 * 6);
            else
                s.in1 = kTile;
        }
        spans.push_back(s);
    }
    return spans;
}

/* A LibRaw instance used only to run xtrans_interpolate on one block buffer. */
class BlockDemosaic : public LibRaw {
  public:
    explicit BlockDemosaic(const libraw_data_t &src) {
        imgdata.idata.filters = src.idata.filters;
        imgdata.idata.colors = src.idata.colors;
        memcpy(imgdata.idata.xtrans, src.idata.xtrans, sizeof imgdata.idata.xtrans);
        memcpy(imgdata.color.rgb_cam, src.color.rgb_cam, sizeof imgdata.color.rgb_cam);
    }
    ~BlockDemosaic() override { imgdata.image = nullptr; /* borrowed, not ours to free */ }

    void run(ushort (*block)[4], int w, int h, int passes) {
        imgdata.image = block;
        imgdata.sizes.width = imgdata.sizes.iwidth = static_cast<ushort>(w);
        imgdata.sizes.height = imgdata.sizes.iheight = static_cast<ushort>(h);
        try {
            xtrans_interpolate(passes);
        } catch (...) {
            imgdata.image = nullptr;
            throw;
        }
        imgdata.image = nullptr;
    }
};

/* Runs body(worker, i) for i in [0, count) on up to `workers` threads (the caller is
 * worker 0). LibRaw's OpenMP regions inside run single-threaded. The first exception
 * stops the remaining work and is rethrown here. */
template <class Body> void run_parallel(int count, int workers, const Body &body) {
    std::atomic<int> next{0};
    std::atomic<bool> failed{false};
    std::exception_ptr error;
    std::mutex error_mutex;
    auto loop = [&](int worker) {
        OmpThreads single(1);
        try {
            for (int i; !failed.load() && (i = next.fetch_add(1)) < count;) body(worker, i);
        } catch (...) {
            std::lock_guard<std::mutex> lock(error_mutex);
            if (!error) error = std::current_exception();
            failed = true;
        }
    };
    std::vector<std::thread> pool;
    for (int w = 1; w < workers; w++) {
        try {
            pool.emplace_back(loop, w);
        } catch (const std::system_error &) {
            break; /* fewer threads: the running workers take the rest */
        }
    }
    loop(0);
    for (auto &t : pool) t.join();
    if (error) std::rethrow_exception(error);
}

class PeLibRaw : public LibRaw {
  public:
    PeLibRaw() { callbacks.interpolate_xtrans_cb = &PeLibRaw::interpolate_xtrans; }

  protected:
    void copy_bayer(unsigned short cblack[4], unsigned short *dmaxp) override {
        if (imgdata.idata.filters == LIBRAW_XTRANS && libraw_internal_data.internal_output_params.shrink &&
            omp().available())
            copy_xtrans_shrunk(cblack, dmaxp);
        else
            LibRaw::copy_bayer(cblack, dmaxp);
    }

  private:
    /* Race 1: LibRaw's copy_bayer, but each task owns whole output rows. Raw rows 2k
     * and 2k+1 are written in order, so where both hold the same colour the later one
     * wins, exactly as in LibRaw's single-threaded loop. */
    void copy_xtrans_shrunk(const unsigned short cblack[4], unsigned short *dmaxp) {
        const libraw_image_sizes_t &s = imgdata.sizes;
        const int rows = std::min<int>(s.height, int(s.raw_height) - s.top_margin);
        const int cols = std::min<int>(s.width, int(s.raw_width) - s.left_margin);
        const int shrink = libraw_internal_data.internal_output_params.shrink;
        const size_t pitch = s.raw_pitch / 2;
        constexpr int kRowsPerTask = 16; /* even: a task never splits an output row */
        const int tasks = (rows + kRowsPerTask - 1) / kRowsPerTask;
        const int workers = std::max(1, std::min(tasks, omp().get_max_threads()));
        std::vector<ushort> worker_max(static_cast<size_t>(workers), 0);
        run_parallel(tasks, workers, [&](int worker, int task) {
            ushort local = worker_max[static_cast<size_t>(worker)];
            const int end = std::min(rows, (task + 1) * kRowsPerTask);
            for (int row = task * kRowsPerTask; row < end; row++) {
                const ushort *src = imgdata.rawdata.raw_image + (row + s.top_margin) * pitch + s.left_margin;
                ushort(*dst)[4] = imgdata.image + static_cast<size_t>(row >> shrink) * s.iwidth;
                const char *pattern = imgdata.idata.xtrans[row % 6];
                for (int col = 0; col < cols; col++) {
                    const int cc = pattern[col % 6];
                    ushort val = src[col];
                    if (val > cblack[cc]) {
                        val -= cblack[cc];
                        local = std::max(local, val);
                    } else {
                        val = 0;
                    }
                    dst[col >> shrink][cc] = val;
                }
            }
            worker_max[static_cast<size_t>(worker)] = local;
        });
        for (ushort m : worker_max) *dmaxp = std::max(*dmaxp, m);
    }

    static void interpolate_xtrans(void *self) {
        static_cast<PeLibRaw *>(static_cast<LibRaw *>(self))->xtrans_blocked();
    }

    /* Same pass count as dcraw_process (the shim never sets user_qual). */
    int xtrans_passes() const {
        int quality = 2 + !libraw_internal_data.internal_output_params.fuji_width;
        if (imgdata.params.user_qual >= 0) quality = imgdata.params.user_qual;
        return quality > 2 ? 3 : 1;
    }

    void check_cancel(int done, int total) {
        if (callbacks.progress_cb &&
            callbacks.progress_cb(callbacks.progresscb_data, LIBRAW_PROGRESS_INTERPOLATE, done, total))
            throw LIBRAW_EXCEPTION_CANCELLED_BY_CALLBACK;
    }

    void xtrans_blocked() {
        const int passes = xtrans_passes();
        const int width = imgdata.sizes.width, height = imgdata.sizes.height;
        if (!omp().available() || width < kTile || height < kTile) {
            /* Single-threaded LibRaw (deterministic), or too small for LibRaw anyway. */
            xtrans_interpolate(passes);
            return;
        }
        const std::vector<Span> rows = split(height), cols = split(width);
        struct Block {
            Span r, c;
        };
        std::vector<Block> phases[4];
        for (size_t i = 0; i < rows.size(); i++)
            for (size_t j = 0; j < cols.size(); j++) phases[(i & 1) * 2 + (j & 1)].push_back({rows[i], cols[j]});

        size_t largest_phase = 0;
        for (auto &p : phases) {
            /* Largest first, so the bigger edge blocks do not end a phase on their own. */
            std::stable_sort(p.begin(), p.end(), [](const Block &a, const Block &b) {
                return (a.r.in1 - a.r.in0) * (a.c.in1 - a.c.in0) > (b.r.in1 - b.r.in0) * (b.c.in1 - b.c.in0);
            });
            largest_phase = std::max(largest_phase, p.size());
        }
        const int workers = static_cast<int>(
            std::min<size_t>(largest_phase, static_cast<size_t>(std::max(1, omp().get_max_threads()))));
        std::vector<std::unique_ptr<BlockDemosaic>> demosaic;
        for (int w = 0; w < workers; w++) demosaic.push_back(std::make_unique<BlockDemosaic>(imgdata));
        std::vector<std::vector<ushort>> buffers(static_cast<size_t>(workers));

        ushort(*image)[4] = imgdata.image;
        for (int p = 0; p < 4; p++) {
            check_cancel(p, 4);
            const std::vector<Block> &blocks = phases[p];
            run_parallel(static_cast<int>(blocks.size()), workers, [&](int worker, int i) {
                const Span r = blocks[static_cast<size_t>(i)].r, c = blocks[static_cast<size_t>(i)].c;
                const int bw = c.in1 - c.in0, bh = r.in1 - r.in0;
                std::vector<ushort> &buf = buffers[static_cast<size_t>(worker)];
                buf.resize(static_cast<size_t>(bw) * bh * 4);
                ushort(*block)[4] = reinterpret_cast<ushort(*)[4]>(buf.data());
                for (int y = 0; y < bh; y++)
                    memcpy(block[y * bw], image[(size_t)(r.in0 + y) * width + c.in0], sizeof *block * bw);
                demosaic[static_cast<size_t>(worker)]->run(block, bw, bh, passes);
                for (int y = r.out0; y < r.out1; y++)
                    memcpy(image[(size_t)y * width + c.out0], block[(y - r.in0) * bw + (c.out0 - c.in0)],
                           sizeof *block * (c.out1 - c.out0));
            });
        }
    }
};

} // namespace

extern "C" libraw_data_t *pe_libraw_new(void) {
    try {
        PeLibRaw *raw = new PeLibRaw();
        return &raw->imgdata; /* libraw_close deletes it through imgdata.parent_class */
    } catch (...) {
        return nullptr;
    }
}

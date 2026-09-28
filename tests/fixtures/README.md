# Test fixtures

## Layout

| Directory | Committed | Contents |
|---|---|---|
| `synthetic/` | yes | `chart.dng` (1200×800 Bayer DNG) and `chart.jpg`, generated procedurally |
| `golden/renderer/` | yes | expected renderer output for the procedural chart + `*.recipe.json` |
| `golden/raw/` | yes | expected output of synthetic DNG → LibRaw → renderer |
| `local/` | **no** (git-ignored) | real camera files and large synthetic files for benchmarks |

Only procedurally generated images are committed. They contain no third-party
content, and `crates/fixtures` regenerates them bit for bit:

```bash
cargo run -p fixtures --release --bin gen-fixtures            # synthetic/
cargo run -p fixtures --release --bin gen-fixtures -- --large # + local/synthetic-24mp.{dng,jpg}
```

## Adding local test photographs

Put any RAW or JPEG files in `tests/fixtures/local/`. The benchmark harness
(`cargo run -p bench --release`) picks up every supported file there automatically.
Never commit photographs you don't hold the rights to.

The Phase 0 baseline used these CC0 (public domain) samples from
[raw.pixls.us](https://raw.pixls.us). `local/manifest.tsv` lists each file's URL and
sha256 so the set can be recreated:

| File | Camera | MP |
|---|---|---|
| `nikon-z6-14bit-lossless.nef` | Nikon Z 6, 14-bit lossless compressed | 24.5 |
| `canon-eos-r6-20mp.cr3` | Canon EOS R6 | 20.2 |
| `sony-a7iii-14bit-compressed.arw` | Sony ILCE-7M3 | 24.2 |
| `sony-a7riv-61mp-14bit-compressed.arw` | Sony ILCE-7RM4 (high resolution) | 61.0 |
| `fujifilm-xt3-compressed.raf` | Fujifilm X-T3 (X-Trans) | 26.0 |
| `ricoh-gr3.dng` | Ricoh GR III (DNG) | 24.2 |

To re-download (about 200 MB):

```bash
cd tests/fixtures/local
while IFS=$'\t' read -r name desc sha url; do curl -sSfL -o "$name" "$url" && echo "$sha  $name" | shasum -a 256 -c -; done < manifest.tsv
```

## Golden images

Golden tests compare renders against stored PNGs with a small tolerance
(1 code for the renderer, 4 codes for the LibRaw path). When output changes
**intentionally**:

```bash
UPDATE_GOLDEN=1 cargo test -p renderer --test golden
UPDATE_GOLDEN=1 cargo test -p app-core --test golden_raw
```

Then review the new PNGs and bump `renderer::RENDERER_VERSION`.

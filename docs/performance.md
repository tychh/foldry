# Performance and resource budgets

## Reproducible workload

Run the release benchmark from the repository root:

```bash
cargo run --release -p foldry-core --example performance_smoke
```

The default workload performs 1,000,000 matcher decisions, scans 5,000 small
files across 100 directories, and writes each archive format with 5,000
128-byte entries plus one streamed 64 MiB entry. Override the sizes with
`FOLDRY_BENCH_MATCHES`, `FOLDRY_BENCH_SMALL_FILES`, and
`FOLDRY_BENCH_LARGE_BYTES`.

## 2026-07-27 Linux baseline

Reference environment: Linux 7.0 x86-64, Intel Core i5-6200U (2 cores/4
threads, 2.30–2.80 GHz), 7.6 GiB RAM, Rust 1.97.1. Results are local development
measurements, not cross-platform guarantees.

| Workload                       |      Cold-cache first run |               Warm repeat |
| ------------------------------ | ------------------------: | ------------------------: |
| Matcher, 1M decisions          |   1.815 s / 550,685 ops/s |   1.105 s / 904,714 ops/s |
| Scanner, 5,100 entries         | 30 ms / 166,325 entries/s | 29 ms / 171,999 entries/s |
| ZIP writer, 67.75 MB input     |                   2.688 s |                   1.251 s |
| TAR.GZ writer, 67.75 MB input  |                    299 ms |                    205 ms |
| TAR.ZST writer, 67.75 MB input |                    187 ms |                     83 ms |
| Whole warm benchmark peak RSS  |                         — |                 5,416 KiB |

The synthetic payload is highly compressible, so format ratios and relative
codec speed must not be generalized to photos, databases, or already-compressed
data.

## Regression thresholds

These thresholds are intentionally tolerant of shared CI and are review
signals, not hard product promises:

- matcher: at least 250,000 decisions/s;
- scanner: at least 40,000 entries/s for the default small-file fixture;
- writers: ZIP under 8 s, TAR.GZ/TAR.ZST under 3 s for the default fixture;
- whole benchmark peak RSS under 64 MiB;
- preview manifest serializer retains its existing fixed 16 KiB buffer at one
  million synthetic entries;
- IPC/preview/log pages remain bounded to at most 1,000 records.

Investigate a repeated breach on the same runner before accepting a change.
Record runner, commit/digest, workload overrides, cold/warm state, elapsed
times, and peak RSS. Do not compare debug builds with this release baseline.

## Large real-world validation

Before release, repeat the benchmark plus an end-to-end run on:

- at least 100,000 small files;
- one incompressible file of at least 4 GiB on a filesystem that supports it;
- a Unicode-heavy tree and a deep-but-supported Windows path;
- a local SSD and one representative network mount.

The acceptance conditions are bounded memory, responsive cancellation, no
partial final archive, and no regression beyond the thresholds after accounting
for storage throughput.

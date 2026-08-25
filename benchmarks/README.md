# Competitor benchmark

The benchmark runs one equivalent rule: every `img` element requires an `alt`
attribute. It measures complete one-shot processes and discards their output only
after each process has serialized it. The three generated fixtures remain separate;
there is no blended score. `fixtures/manifest.json` records their exact byte sizes,
properties, expected finding counts, and SHA-256 digests.

Build and generate fixtures:

```sh
cargo build --release --bins
cargo run --release --bin generate_bench_fixtures
```

Enable only installed, correctness-checked competitors in `tools.json`, then run:

```sh
cargo run --release --bin benchmark
```

Use `cargo run --release --bin benchmark -- benchmarks/tools.json --smoke`
for one untimed-warmup-free process per tool and fixture before a full run.

Raw samples and machine metadata are written to `results/latest.json`. The small
fixture has no warmups to emphasize process startup. The mixed and large fixtures
use five warmups and at least 20 measured one-shot processes. Tool order is
deterministically shuffled in every round to distribute thermal and background-load
bias. A competitor qualifies
only when it reports 0, 256, and 0 missing-alt findings respectively.

The empty `fixtures/.markuplintrc` is intentional. Markuplint 4.18.3 otherwise
merges its recommended preset when a configuration is supplied only through the
CLI; the discoverable empty file suppresses that default before the explicit
benchmark configuration is merged.

`tools-headline.json` reruns the two fastest tools after performance changes. Preserve
the preceding all-tool evidence before running it; do not substitute the head-to-head
file for the full competitor record.

## Measured result

On the recorded 8-way Apple ARM machine, the final build reached 3.45 ms, 6.87 ms,
and 57.10 ms median on the small, mixed, and large fixtures. It was 21.79× faster
than Biome on the small fixture, at least 26.37× faster than the fastest full-matrix
competitor on the mixed fixture, and 1.55× faster than Biome on the large fixture.
Peak resident memory on the 16 MiB input was 18.08 MiB. See `results/summary.json`
for the machine-readable comparison and the evidence filenames.

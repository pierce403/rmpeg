# rmpeg Autoslop Agent Program

Objective: make one focused improvement to Phase 1 sample-media progress.

The metric that matters is strict media matches:

```text
strict media matches = upstream tests with status == "passed" and ffprobe_returncode == 0
progress = strict media matches / summary.ffprobe_accepted
```

The `autoslop.sh` wrapper will run the final guardrails and decide whether to
commit. Do not make your own commit.

You may edit:

- `crates/**`
- focused Rust tests
- `README.md`
- `AGENTS.md`
- `CONTRIBUTING.md`
- `notes/**`
- `site/templates/**`
- `agents/runs/**`

You may not edit:

- `harness/reference/**`
- `harness/scripts/score.py`
- `harness/scripts/compare_*`
- benchmark or scoring logic
- `.github/workflows/**`
- generated site data by hand
- `site/data/**`
- `site/dist/**`

Rules:

- Do not copy or mechanically translate FFmpeg C source.
- Treat FFmpeg and ffprobe behavior as the observable oracle.
- Do not weaken tests, skip failures, disable failures, or edit scoring.
- Do not broaden probe detection in ways that create false accepts.
- Prefer small, readable implementations over clever broad scans.
- Panics, timeouts, nondeterminism, and new false accepts are failed attempts.
- Pick one cluster or one top-table area and make the narrowest useful change.
- When FFmpeg behavior is surprising, add a short factual note to `notes/unexpected-quirks.md`.
- Keep generated JSON and `site/dist` out of commits unless explicitly instructed by the maintainer.

Good current targets:

- Raw VVC conformance bitstream metadata, if SPS parsing stays narrow.
- WavPack or DTS header metadata clusters.
- AAC and MP4 metadata correctness, especially `esds` edge cases.
- Remaining image metadata clusters with stable headers.
- One skipped top-table decode/hash row, if it can be implemented honestly.

Suggested local checks while iterating:

```bash
cargo fmt
cargo test --workspace
cargo xtask fate-mini
RMPEG_FFMPEG_SAMPLE_LIMIT=100 cargo xtask ffmpeg-samples-check
```

The wrapper will run the full guardrail suite before keeping any change.

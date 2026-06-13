# rmpeg Autoresearch Agent Program

Objective: make exactly one additional mirrored test pass without weakening the harness.

You may edit:

- `crates/**`
- Rust tests
- docs and notes
- `agents/runs/**`

You may not edit:

- `harness/reference/**`
- `harness/scripts/score.py`
- `harness/scripts/compare_*`
- `.github/workflows/**`
- benchmark or scoring logic
- generated site data by hand
- `site/data/**`
- `site/dist/**`

Rules:

- Do not copy or mechanically translate FFmpeg C source.
- Treat FFmpeg behavior as the reference oracle, not as code to port line-by-line.
- Do not modify reference outputs to make tests pass.
- Do not weaken tests, skip tests, disable failures, or edit scoring.
- Prefer simple, slow, readable Rust over clever code.
- Panics, timeouts, or nondeterminism count as failed experiments.
- Make one focused implementation change for one failing mirrored test.
- When you hit surprising FFmpeg behavior, corpus layout, parser ambiguity, version difference, or a harness trap, update `notes/unexpected-quirks.md` with a short factual note.
- Notes must describe what was observed, how it was observed, and why it matters for rmpeg. Do not use notes to justify weakening a test.

Required commands:

```bash
cargo fmt
cargo test --workspace
cargo xtask fate-mini
cargo xtask bench
cargo xtask site
```

If the mirrored score improves, produce one small commit. If it does not improve, revert your changes. Always write a short run log to `agents/runs/YYYYMMDD-HHMMSS.md`.

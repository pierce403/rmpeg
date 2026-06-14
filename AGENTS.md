# AGENTS.md - Instructions for Coding Agents

This is the canonical operating guide for agents working on rmpeg. If another
agent harness expects a different instruction filename, keep this file as the
source of truth and make the other file point here or summarize this file.

## Self-Improvement Directive

When you learn something durable about this repo, improve the system that helps
the next agent. Do not rely on chat context to carry important lessons forward.

Record:

- verified commands and their expected outputs
- surprising FFmpeg or FATE corpus behavior
- parser ambiguity and false-accept risks
- local workflow pitfalls
- contributor or maintainer preferences
- instructions that became outdated

Use the right place:

- `AGENTS.md`: stable operating procedure for future agents
- `notes/unexpected-quirks.md`: short factual notes about media, FFmpeg, corpus, or harness quirks
- `agents/runs/YYYYMMDD-HHMMSS.md`: dated autoresearch attempt summaries
- `CONTRIBUTING.md`: human-facing contribution priorities and acceptance criteria

The goal is recursive self-improvement: every useful run should leave rmpeg
easier to improve than it was before.

## Project Objective

rmpeg is a Rust media stack grown by differential testing against FFmpeg.
FFmpeg is the behavior oracle. Do not copy, port, or mechanically translate
FFmpeg C source.

The current project phase is Phase 1 compatibility: make `rmpeg-probe` and
eventually `rmpeg` successfully inspect and decode as much of the upstream
FFmpeg sample media set as possible while honestly reporting failures.

Phase 2 is optimization. Do not spend major effort on speed until Phase 1
coverage is much stronger, except to keep benchmarks running and truthful.

## Current Metric

The metric that matters right now is Phase 1 sample-media progress:

```text
strict media matches = upstream tests with status == "passed" and ffprobe_returncode == 0
progress = strict media matches / summary.ffprobe_accepted
```

The public site currently reports the deployed snapshot. Local runs may differ
from GitHub Actions because `ffprobe` versions differ. Treat the current oracle
snapshot as data, not as a hardcoded expected total.

Current local full-corpus snapshot from 2026-06-13 after WavPack metadata probing:

```text
1257 / 2178 strict media matches = 57.7%
1589 / 2511 total corpus passes, including files both tools reject
0 corpus errors
1 known false accept: aac/usac/Ext_2_c1_Ln_0x03.mp4
```

Commit and push when a focused change improves Phase 1 progress by at least 1
absolute percentage point and the local gate passes.

## Non-Negotiable Rules

- Do not weaken tests, skip failures, or edit scoring to improve the score.
- Do not modify generated FFmpeg references to make rmpeg pass.
- Do not hand-edit generated `site/data/**` or `site/dist/**` for a score.
- Do not broaden format detection in ways that create false accepts.
- Panics, timeouts, and nondeterminism count as failed experiments.
- Prefer simple, slow, readable Rust over clever code.
- Keep unrelated refactors out of progress commits.
- Never use destructive git commands unless the human explicitly asks.

## Protected Paths

Do not edit these to improve a result:

- `harness/reference/**`
- `harness/scripts/compare_*`
- `harness/scripts/score.py`
- benchmark or scoring logic
- `.github/workflows/**` unless the task is explicitly CI or Pages work
- generated `site/data/**`
- generated `site/dist/**`

It is fine to regenerate ignored local data with `cargo xtask ...`; do not
commit generated data unless the repo already tracks that file and the change is
part of the requested task.

## Standard Workflow

1. Check repo state with `git status --short`.
2. Read `README.md`, `CONTRIBUTING.md`, this file, and relevant notes.
3. Pick one focused improvement tied to a failing corpus cluster.
4. Inspect FFmpeg behavior with `ffprobe`, not FFmpeg source.
5. Implement the smallest parser, demuxer, decoder, or harness-neutral fix.
6. Add or update Rust unit tests for local parser behavior.
7. Update `notes/unexpected-quirks.md` for surprising behavior.
8. Run the local gate.
9. Regenerate the site from real JSON.
10. Commit and push only when the progress threshold or requested task is met.
11. Watch CI and Pages after pushing.
12. Verify `https://rmpeg.org/` and `https://rmpeg.org/og-card.png` after Pages deploys.

## Verified Commands

Use these from the repository root:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo xtask samples
cargo xtask reference
cargo xtask fate-mini
cargo xtask ffmpeg-samples-check
cargo xtask bench
cargo xtask site
```

Useful metric checks:

```bash
jq '[.tests[] | select(.status=="passed" and .ffprobe_returncode==0)] | length' site/data/upstream-samples.json
jq '.summary' site/data/upstream-samples.json
jq -r '.tests[] | select(.ffprobe_returncode != 0 and .rmpeg_returncode == 0) | .name + " :: " + .details' site/data/upstream-samples.json
```

Use `RMPEG_FFMPEG_SAMPLE_LIMIT=100 cargo xtask ffmpeg-samples-check` only as a
smoke test. Full progress claims require the full corpus check.

## Autoresearch Loop

For autonomous improvement, use `agents/autoresearch.py` and
`agents/program.md`. The intended loop is:

1. Record baseline score and current commit.
2. Make exactly one additional mirrored or corpus-backed behavior pass.
3. Preserve strict oracle behavior.
4. Run the required commands.
5. Commit only if the score improves and forbidden files were not touched.
6. Revert if the score does not improve.
7. Write a dated run log.

Do not hardcode a specific external agent vendor. Use `AGENT_CMD`:

```bash
AGENT_CMD="codex exec --full-auto agents/program.md" python3 agents/autoresearch.py
```

For a corpus-metric attempt, prefer `./autoslop.sh`. It uses
`agents/autoslop.md`, requires a clean tree, auto-detects `codex`, `claude`, or
`aider` when `AGENT_CMD` is not set, and keeps a change only when the Phase 1
strict-media metric improves enough without protected path edits, corpus errors,
new false accepts, or mirrored-score regression. It is intentionally one-shot,
not an infinite loop.

Useful invocations:

```bash
AGENT_CMD="codex exec --full-auto agents/autoslop.md" ./autoslop.sh
AUTOSLOP_MIN_PERCENT_GAIN=0 AUTOSLOP_MIN_STRICT_GAIN=1 ./autoslop.sh
AUTOSLOP_PUSH=1 ./autoslop.sh
```

For PR maintenance, use `./automerge.sh` to inspect open pull requests. It is
dry-run by default and requires `./automerge.sh --apply` before merging. The
default policy requires a non-draft PR, an approving review, passing checks,
merge state `CLEAN` or `HAS_HOOKS`, and no protected-path edits.

## Current High-Value Work

Good Phase 1 targets are clusters where ffprobe accepts many files and rmpeg
rejects or mismatches them:

- image metadata probers with stable headers before decode work
- AAC and MP4 metadata correctness, especially `esds` edge cases
- DTS header metadata clusters
- remaining JPEG/JPEG 2000 container edge cases once false accepts are controlled
- compressed audio metadata where headers are small and well-scoped

Avoid work that only makes rmpeg accept more files while producing wrong stream
metadata. A reject is better than a false claim of support.

## Coding Conventions

- Keep parser modules small and format-specific.
- Put dispatch guards in `crates/rmpeg-format/src/probe.rs`.
- Return ffprobe-compatible normalized metadata names, including pipe demuxer
  names such as `png_pipe`, `bmp_pipe`, or `sgi_pipe` when ffprobe does.
- Validate magic numbers and essential header fields before accepting input.
- Use checked bounds and explicit `UnexpectedEof` errors.
- Add focused unit tests for every parser edge you rely on.
- Keep generated media out of git unless it is intentionally tiny and already part of the harness.

## Collaboration Notes

- The maintainer wants local tests run before pushing.
- Push progress commits whenever Phase 1 improves by at least 1 absolute percentage point.
- Keep the public site and OpenGraph card current after pushes.
- Keep notes about unexpected quirks as they are discovered.
- Be direct and specific in status updates.

## Periodic Reflection

Occasionally review agent-process advice such as `https://recurse.bot/`.
Adopt only the parts that help this repo: canonical instructions, durable
memory, verified commands, and small compounding improvements. Do not import
generic customs that conflict with rmpeg's rules or the maintainer's workflow.

# Contributing to rmpeg

rmpeg is an experimental Rust media stack tested against FFmpeg. The project is
not trying to copy FFmpeg internals. It uses FFmpeg and ffprobe as behavior
oracles, then grows small Rust implementations that match observable behavior.

## What We Are Chasing Now

The current phase is Phase 1 compatibility.

Primary metric:

```text
strict media matches / ffprobe-accepted files in the upstream FFmpeg sample corpus
```

Locally, compute it with:

```bash
cargo xtask ffmpeg-samples-check
jq '[.tests[] | select(.status=="passed" and .ffprobe_returncode==0)] | length' site/data/upstream-samples.json
jq '.summary.ffprobe_accepted' site/data/upstream-samples.json
```

The public site reports the latest deployed GitHub Actions snapshot. Local
numbers can differ slightly because ffprobe builds differ.

Current local full-corpus snapshot from 2026-06-13 after APE and raw AMR-NB metadata probing:

```text
1313 / 2178 strict media matches = 60.3%
1645 / 2511 total corpus passes, including files both ffprobe and rmpeg reject
0 corpus errors
1 known false accept: aac/usac/Ext_2_c1_Ln_0x03.mp4
```

Secondary metrics:

- no panics
- no timeouts
- no nondeterministic output
- no new false accepts where ffprobe rejects input but rmpeg accepts it
- mirrored tests stay passing
- benchmark JSON and the site remain honest

## Best Contributions Right Now

High-value contributions make a real corpus cluster pass without weakening the
harness. Good starting points:

- Add header-only metadata probers for formats with stable signatures and clear
  dimensions or stream metadata.
- Improve AAC-in-MP4 metadata parsing from `esds` rather than sample-entry
  defaults.
- Improve raw H.264 and MP4 probing without scanning arbitrary binary payloads.
- Add narrow metadata probing for DTS or remaining common container clusters.
- Convert known quirks into focused Rust tests.
- Improve generated site clarity when it reflects real JSON output.

Current large failing clusters can be found with:

```bash
jq -r '.tests[] | select(.status!="passed" and .ffprobe_returncode==0) | .name' site/data/upstream-samples.json \
  | sed 's#/.*##' \
  | sort \
  | uniq -c \
  | sort -nr \
  | head -40
```

## What Not To Do

- Do not copy or mechanically translate FFmpeg C source.
- Do not edit references, comparison scripts, or scoring to improve a result.
- Do not skip failing cases.
- Do not hand-edit generated `site/data/**` or `site/dist/**` for progress.
- Do not make broad probe detection that accepts random binary files.
- Do not claim decode support for formats that only have metadata probing.

## Local Setup

Required tools:

- Rust stable
- Python 3
- FFmpeg and ffprobe
- hyperfine
- git, make, and rsync for the upstream FFmpeg sample corpus

On Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y ffmpeg git hyperfine make python3 rsync
```

Sync the upstream sample corpus:

```bash
cargo xtask ffmpeg-samples-sync
```

This stores sample cache data under `.cache/ffmpeg/` by default.

## Required Checks

Before a contribution is ready, run:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo xtask fate-mini
cargo xtask ffmpeg-samples-check
cargo xtask bench
cargo xtask site
```

For quick iteration, you may use:

```bash
RMPEG_FFMPEG_SAMPLE_LIMIT=100 cargo xtask ffmpeg-samples-check
```

Do not use a limited run as proof of a Phase 1 percentage gain.

## Automation

`./autoslop.sh` runs one guarded autonomous improvement attempt. It is allowed to
keep a commit only when Phase 1 strict media progress improves, protected paths
stay untouched, the mirrored score does not regress, corpus errors remain zero,
and false accepts do not increase.

`./automerge.sh` reviews open pull requests through `gh`. It is dry-run by
default; use `./automerge.sh --apply` to merge PRs that satisfy the conservative
policy.

## Contribution Shape

Good changes are small and measurable:

1. Identify a failing sample cluster.
2. Inspect observable ffprobe behavior.
3. Implement the narrowest Rust behavior that matches the oracle.
4. Add focused unit tests.
5. Run the full local gate.
6. Update `notes/unexpected-quirks.md` if anything surprising came up.
7. Regenerate the site from real JSON.
8. Report the before and after strict media match count.

If the change improves Phase 1 progress by at least 1 absolute percentage point,
it is a good candidate for an immediate progress commit and push.

## Documentation Expectations

- Update `README.md` when supported behavior changes.
- Update `site/templates/index.html` when public status wording changes.
- Update `notes/unexpected-quirks.md` for surprising FFmpeg or corpus behavior.
- Update `AGENTS.md` when the agent workflow itself improves.

Keep docs honest. If only metadata probing works, say metadata probing. If decode
is missing, say decode is missing.

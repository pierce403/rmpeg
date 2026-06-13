# rmpeg

rmpeg is an experimental Rust media stack grown by differential testing against FFmpeg.

It is not FFmpeg-compatible yet. The MVP supports a narrow media vertical slice:

- RIFF/WAVE header parsing
- `fmt ` and `data` chunk discovery
- PCM signed 16-bit little-endian audio
- PCM unsigned 8-bit audio normalized to signed 16-bit samples for hashing
- mono and stereo
- sample rate, channels, bits per sample, data size, and duration estimate
- a framemd5-like decode/hash path for PCM data
- MP3 frame-header metadata probing
- FLAC STREAMINFO metadata probing
- Ogg/Opus and Ogg/Vorbis header metadata probing
- MP4/MOV track metadata probing for H.264 video and AAC audio
- IVF metadata probing for VP8, VP9, and AV1 video

Compressed decode is not implemented yet. MP3, FLAC, Opus, Vorbis, AAC, H.264, VP8, VP9, and AV1 support is probe-level metadata only.

FFmpeg is used as the behavior oracle. This project does not copy or mechanically translate FFmpeg C source.

## Roadmap

Phase 1 is compatibility: make rmpeg successfully inspect and eventually decode as much of the upstream FFmpeg sample media set as possible, with the site reporting real progress from `site/data/upstream-samples.json`.

Phase 2 is optimization. Once Phase 1 is no longer the main blocker, the site should show which older FFmpeg codec paths rmpeg is faster than, using the benchmark JSON instead of hand-written claims.

## Commands

Generate deterministic tiny WAV fixtures:

```bash
cargo xtask samples
```

Generate FFmpeg and ffprobe reference outputs:

```bash
cargo xtask reference
```

Run mirrored correctness tests and write `site/data/correctness.json`:

```bash
cargo xtask fate-mini
```

Sync the upstream FFmpeg FATE sample corpus with FFmpeg's own `make fate-rsync` target:

```bash
cargo xtask ffmpeg-samples-sync
```

Probe every regular file in the synced FFmpeg sample corpus with `ffprobe` and `rmpeg-probe`,
then write `site/data/upstream-samples.json`:

```bash
cargo xtask ffmpeg-samples-check
```

Run both upstream sample steps:

```bash
cargo xtask ffmpeg-samples
```

Run hyperfine benchmarks and write benchmark JSON:

```bash
cargo xtask bench
```

Render the static status page:

```bash
cargo xtask site
```

Open the generated page at:

```text
site/dist/index.html
```

## CLIs

Probe a WAV file:

```bash
cargo run -p rmpeg-probe -- harness/samples/tiny.wav
```

Hash decoded PCM frames:

```bash
cargo run -p rmpeg-cli -- decode harness/samples/tiny.wav --framemd5
```

Release binaries are written to `target/release/rmpeg-probe` and `target/release/rmpeg`.

## Autoresearch

Run one external dev-agent attempt:

```bash
AGENT_CMD="codex exec --full-auto agents/program.md" python3 agents/autoresearch.py
```

The autoresearch driver:

1. Records the current commit.
2. Runs the baseline mirrored score.
3. Invokes the command in `AGENT_CMD`.
4. Runs the same score again.
5. Commits the change only if the score improves and forbidden files were not touched.
6. Reverts the attempt otherwise.
7. Writes a short run log in `agents/runs/`.

Forbidden paths include generated FFmpeg references, comparison scripts, scoring logic, CI workflows, and generated site data.

## Current Result

The current mirrored suite has 24 tests:

- 12 probe tests
- 7 decode/hash tests
- 5 skipped compressed decode/hash tests
- no failing tests against local FFmpeg references

## Required Tools

Linux with:

- Rust stable
- Python 3
- FFmpeg and ffprobe
- hyperfine
- git, make, and rsync for `cargo xtask ffmpeg-samples`

On Ubuntu, FFmpeg and hyperfine are available through apt on recent releases:

```bash
sudo apt-get update
sudo apt-get install -y ffmpeg git hyperfine make python3 rsync
```

The upstream FFmpeg sample command stores local artifacts under `.cache/ffmpeg/` by default.
Override locations with:

```bash
RMPEG_FFMPEG_SOURCE_DIR=/path/to/ffmpeg-src \
RMPEG_FFMPEG_BUILD_DIR=/path/to/ffmpeg-build \
RMPEG_FFMPEG_SAMPLES_DIR=/path/to/fate-suite \
cargo xtask ffmpeg-samples
```

Use `RMPEG_FFMPEG_REPO` and `RMPEG_FFMPEG_REF` to point at a different FFmpeg remote or ref.
Use `RMPEG_FFMPEG_SAMPLE_LIMIT=100 cargo xtask ffmpeg-samples-check` for a quick local smoke run.
The `upstream-samples` GitHub Actions workflow can also be triggered manually to run this corpus
check and upload `site/data/upstream-samples.json` plus a rendered site artifact.

# rmpeg

rmpeg is an experimental Rust media stack grown by differential testing against FFmpeg.

It is not FFmpeg-compatible yet. The MVP only supports a narrow WAV/PCM vertical slice:

- RIFF/WAVE header parsing
- `fmt ` and `data` chunk discovery
- PCM signed 16-bit little-endian audio
- PCM unsigned 8-bit audio normalized to signed 16-bit samples for hashing
- mono and stereo
- sample rate, channels, bits per sample, data size, and duration estimate
- a framemd5-like decode/hash path for PCM data

FFmpeg is used as the behavior oracle. This project does not copy or mechanically translate FFmpeg C source.

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

The current WAV mirrored suite has 14 tests:

- 7 probe tests
- 7 decode/hash tests
- all passing against local FFmpeg references

## Required Tools

Linux with:

- Rust stable
- Python 3
- FFmpeg and ffprobe
- hyperfine

On Ubuntu, FFmpeg and hyperfine are available through apt on recent releases:

```bash
sudo apt-get update
sudo apt-get install -y ffmpeg hyperfine python3
```

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
- raw AC-3 and E-AC-3 metadata probing
- raw AMR-NB metadata probing
- FLAC STREAMINFO metadata probing
- Monkey's Audio / APE metadata probing
- Ogg/Opus, Ogg/Vorbis, Ogg/Theora, and Ogg/VP8 header metadata probing, including truncated final-page duration handling
- FLV H.264/AAC sequence-header metadata probing
- MP4/MOV track metadata probing for H.264/HEVC/MPEG-4 video, AAC/AMR/Vorbis/QuickTime PCM audio, and common QuickTime video sample entries
- fragmented MP4/MOV duration probing from `moof`/`traf` timing and HEIF/HEIC still-image metadata from item properties
- CAF audio metadata probing for observed AAC, Opus, and PCM fixtures
- MPEG-TS program metadata probing for observed MPEG-2 video, H.264, HEVC, VVC, MPEG audio, AAC LATM, AC-3, and Opus fixtures
- EA VP6 metadata probing for observed Electronic Arts VP6 files
- raw DNxHD/DNxHR frame metadata probing
- narrow MXF metadata probing for DNXUC FATE fixtures
- ASF/WMA Lossless, WMAPro, WMA Voice, MSS2, and G2M metadata probing from ASF header objects
- AEA/ATRAC1 and ATRAC3-in-WAV metadata probing for observed FATE fixtures
- ALP and APM ADPCM metadata probing for observed FATE fixtures
- QOA audio metadata probing
- TTA, OptimFROG, TAK, MLP, and TrueHD metadata probing
- Bink video/audio metadata probing for observed FATE fixtures
- QCP, NIST Sphere, Sony Wave64 PCM, raw ADP/DTK, and Westwood AUD audio metadata probing for observed FATE fixtures
- Nintendo BFSTM/BRSTM stream metadata probing
- extension-gated raw G.722 and G.723.1 audio metadata probing
- extension-gated PP_BNK soundbank metadata probing
- extension-gated CDXL video/audio metadata probing
- GIF image/animation metadata probing
- DPX image metadata probing
- FLIC animation and RenderWare TXD image metadata probing
- FITS image metadata probing
- IFF ILBM/PBM image and 8SVX audio metadata probing
- narrow JPEG XL metadata probing for observed raw and boxed codestream headers
- Creative VOC and Creative ADPCM WAV metadata probing
- additional compressed WAV metadata probing for observed GSM, OKI ADPCM, Sanyo ADPCM, MSN Siren, TrueSpeech, and WMA v2 fixtures
- RealMedia metadata probing for observed RealAudio/RealVideo/SIPR/Cook fixtures
- GameCube RSD audio metadata probing for observed RADP/GADP fixtures
- raw Advanced Profile VC-1 and extension-gated VC-1 RCV wrapper metadata probing
- SMJPEG, Bethesda VID, and VMD metadata probing for observed game-media fixtures
- TTY/ANSI text demuxer metadata for observed ffprobe-accepted text reports
- IVF metadata probing for VP8, VP9, and AV1 video
- raw Annex B H.264 SPS metadata probing
- raw MPEG-4 Visual VOL metadata probing
- raw MPEG video sequence-header metadata probing
- binary PNM image metadata probing for PBM, PGM, and PPM files
- DDS image metadata probing
- PNG, APNG, and BMP image metadata probing
- SGI image metadata probing
- PSD image metadata probing
- JPEG/MJPEG image metadata probing
- WebP image metadata probing
- Sun Raster image metadata probing
- OpenEXR image metadata probing
- JPEG 2000 codestream metadata probing
- TGA image metadata probing for files with a TGA 2.0 footer
- TIFF image metadata probing
- conservative Matroska/WebM track metadata probing, including Opus/Vorbis, ProRes, rawvideo, TTA, and subtitle-only metadata normalization
- MP4 AAC/ALS `esds` metadata probing for AudioSpecificConfig sample rate, channels, and ALS bit depth
- narrow RIFF/AVI video metadata probing for UtVideo fixtures
- broader RIFF/AVI codec-tag metadata probing for observed FATE video fourccs
- narrow DTS metadata probing for DTS-HD, raw DTS, and DTS PES in MPEG-TS fixtures
- content-signature subtitle text probing for common standalone subtitle formats
- raw HEVC Annex B bitstream metadata probing
- narrow raw VVC Annex B bitstream metadata probing, including observed compact SPS and RPR fixtures
- WavPack metadata probing from raw blocks and Matroska tracks
- BRender PIX and Alias PIX image metadata probing
- XBM image and Westwood VQA video/audio metadata probing

Compressed decode is not implemented yet. MP3, AC-3, E-AC-3, AMR-NB, FLAC, APE, Opus, Vorbis, AAC, AMR-WB, CAF, QCP, NIST Sphere, W64, raw ADP/DTK, QOA, WavPack, WMA Lossless, WMAPro, WMA Voice, ATRAC1, ATRAC3, ALP/APM ADPCM, RealAudio/RealVideo, TTA, OptimFROG, TAK, MLP, TrueHD, Bink, Westwood AUD, G.722, G.723.1, PP_BNK, CDXL, VOC, compressed WAV tags, SMJPEG, Bethesda VID, VMD, BFSTM/BRSTM, RSD, H.264, HEVC, VVC, VC-1, raw MPEG-4 Visual, raw MPEG video, DNxHD/DNxHR, DNXUC, G2M, Hap, ProRes, QuickTime Animation, DV, VP8, VP9, AV1, FLV, Matroska/WebM, MPEG-TS, EA VP6, Westwood VQA, HEIF/HEIC, subtitles, DDS, GIF, DPX, FLIC, TXD, FITS, IFF, JPEG XL, PNG/APNG, BMP, BRender PIX, Alias PIX, XBM, SGI, PSD, JPEG/MJPEG, WebP, Sun Raster, OpenEXR, JPEG 2000, TGA, TIFF, and PNM image support is probe-level metadata only.

FFmpeg is used as the behavior oracle. This project does not copy or mechanically translate FFmpeg C source.

## Roadmap

Phase 1 is compatibility: make rmpeg successfully inspect and eventually decode as much of the upstream FFmpeg sample media set as possible, with the site reporting real progress from `site/data/upstream-samples.json`.

Current Phase 1 strict corpus progress is 1851 of 2178 FFmpeg-accepted samples, or 84.986%, on the local upstream sample report.

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

## Autoslop

Run one autonomous Phase 1 improvement attempt with corpus guardrails:

```bash
AGENT_CMD="codex exec --full-auto agents/autoslop.md" ./autoslop.sh
```

If `AGENT_CMD` is unset, `autoslop.sh` tries installed local harnesses in this order:
`codex`, `claude`, then `aider`.

`autoslop.sh` requires a clean working tree, records the baseline strict media
match count, runs one agent attempt, reruns the local gate, blocks protected
path edits, and keeps the attempt only if strict media progress improves without
new corpus errors, new false accepts, or mirrored-score regression. The coding
harness starts only after the baseline strict media line is printed; the long
`ffmpeg-samples-check` step prints corpus progress while it runs. By default it
requires at least a 1.0 absolute percentage point Phase 1 gain. Override with:

```bash
AUTOSLOP_MIN_PERCENT_GAIN=0 AUTOSLOP_MIN_STRICT_GAIN=1 ./autoslop.sh
```

Use `RMPEG_FFMPEG_SAMPLE_LIMIT=100` only for smoke-testing the loop shape.
Set `AUTOSLOP_PUSH=1` to push a kept commit to `origin/main`.

## Automerge

Preview pull requests that look safe to merge:

```bash
./automerge.sh
```

Actually merge candidates:

```bash
./automerge.sh --apply
```

The script uses `gh`, requires passing checks and an approving review by default,
skips draft or blocked PRs, and refuses protected-path changes unless
`AUTOMERGE_ALLOW_PROTECTED=1` is set.

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

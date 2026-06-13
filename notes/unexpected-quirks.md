# Unexpected Quirks

## 2026-06-13

- Upstream FFmpeg FATE samples are not just media fixtures. The synced corpus also contains files such as `md5sum`, `README.txt`, `.pcm`, `.s16`, and license text. For the full-corpus probe report, a file is a pass when both `ffprobe` and `rmpeg-probe` reject it.

- The upstream FATE sample corpus is large enough to be treated as local cache, not source. On this machine `make fate-rsync` synced 2,511 regular files and about 1.2 GB into `.cache/ffmpeg/fate-suite/`.

- ADTS AAC and MP3 both start with `0xff` sync-shaped bytes. A raw AAC file can be misdetected as MP3 if probe dispatch only checks the broad MPEG sync mask. Check ADTS before MP3 and avoid dispatching MP3 from arbitrary sync-looking bytes.

- Raw PCM-like `.s16` samples can contain byte patterns that look like MP3 frame syncs. `ffprobe` rejects these files without an explicit demuxer hint, so `rmpeg-probe` should not scan arbitrary leading binary data until it finds a plausible MP3 frame.

- Some WAV files in FATE use `WAVE_FORMAT_EXTENSIBLE` even when the payload is plain PCM. The parser needs to inspect the subformat GUID and then normalize supported PCM subformats back to ordinary PCM metadata.

- MP4 audio sample-entry defaults are often not the audio stream metadata FFmpeg reports. AAC-in-MP4 may need the `esds` AudioSpecificConfig for sample rate, channel count, SBR extension rate, and codec family. Treat the sample-entry fixed fields as fallback metadata.

- The same FATE corpus can produce different aggregate counts between local Linux and GitHub Actions because the installed `ffprobe` build/version differs. Use the report as an honest current-oracle snapshot, not as a hardcoded expected total.

- Some advanced MPEG-4 audio object types, especially USAC, should be rejected until rmpeg can parse them deliberately. Accepting them as generic AAC can make rmpeg appear to support files that the local `ffprobe` oracle rejects.

- IVF stores its frame timing as denominator then numerator in the file header. `ffprobe` reports duration as `frame_count * numerator / denominator`; treating the fields as a frame rate directly flips the ratio.

- FFmpeg reports standalone binary PNM images as pipe demuxers such as `pgm_pipe`, not just `pgm`. Matching ffprobe means the document format should preserve the `_pipe` demuxer name while the stream codec stays `pgm`, `ppm`, or `pbm`.

- Raw PCM fixtures can accidentally contain H.264 Annex B start-code patterns. H.264 probing should require a plausible opening sequence and a parseable SPS near the front of the file instead of scanning the entire payload for `0x00000167`.

- Like PNM, standalone DDS images are reported by ffprobe with a pipe demuxer format (`dds_pipe`) and stream codec `dds`. The DDS dimensions live in the fixed 124-byte header immediately after the magic.

- Standalone PNG and BMP images also use ffprobe pipe demuxer names (`png_pipe`, `bmp_pipe`) while the video stream codec is just `png` or `bmp`. Animated PNG keeps the same PNG signature but ffprobe reports format `apng` and stream codec `apng` when an `acTL` chunk appears before image data. BMP dimensions are split between old OS/2 CORE headers with 16-bit dimensions and later DIB headers with signed 32-bit dimensions.

- SGI RGB images use big-endian header fields. ffprobe reports standalone files as `sgi_pipe` with stream codec `sgi`, and both uncompressed and RLE files share the same width/height header layout.

- WebP is RIFF, but not WAVE. Probe dispatch must check both `RIFF` and `WAVE` before handing a file to the WAV parser, otherwise RIFF WebP files fail before WebP gets a chance. Animated WebP fixtures in the FATE corpus can carry the VP8X animation flag while local ffprobe reports 0x0 stream dimensions, so rmpeg mirrors that conservative metadata for now.

- Most standalone JPEG FATE images normalize as format `image2`, stream codec `mjpeg`, and a 0.04 second duration, but `jpg/jpg-8930-1.jpg` currently expects `jpeg_pipe` with 0.0 duration. That demuxer distinction is still an explicit remaining failure instead of being hidden.

- PSD and Sun Raster files are pipe demuxers in ffprobe output: `psd_pipe` with stream codec `psd`, and `sunrast_pipe` with stream codec `sunrast`. Both need strict magic and dimension validation before accepting input.

- OpenEXR files use a little-endian magic/version header followed by null-terminated attributes. ffprobe reports standalone files as `exr_pipe` with stream codec `exr`. When `displayWindow` differs from `dataWindow`, ffprobe reports display-window dimensions, so rmpeg should prefer `displayWindow` and fall back to `dataWindow`.

- Raw JPEG 2000 codestreams start with SOC `ff4f` followed by a SIZ marker. ffprobe reports standalone codestreams as `j2k_pipe` with stream codec `jpeg2000` and 0.0 normalized duration. For subsampled codestreams, ffprobe reports the first component grid dimensions, so use `ceil(Xsiz / XRsiz) - ceil(XOsiz / XRsiz)` and the same formula for Y. MXF-wrapped JPEG 2000 remains a container problem, not a raw codestream probe.

- The TGA FATE samples carry the TGA 2.0 `TRUEVISION-XFILE` footer even though TGA has no leading magic number. Local ffprobe reports them as `image2` with stream codec `targa` and a 0.04 second still-image duration, so rmpeg only probes footered TGA files for now to avoid broad false accepts.

- Standalone TIFF FATE files normalize as `tiff_pipe` with stream codec `tiff`. Local ffprobe omits per-stream duration for these files; the comparison harness normalizes missing video duration to 0.0.

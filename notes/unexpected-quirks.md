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

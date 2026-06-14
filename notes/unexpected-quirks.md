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

- Matroska/WebM files are EBML, but EBML magic alone is too broad. Require `DocType` `matroska` or `webm`, parse only `Tracks` metadata, and require at least one `Cluster` before accepting. FATE has WebM DASH `.hdr` header fragments that carry valid EBML/track metadata while local ffprobe rejects them as standalone inputs.

- Raw HEVC conformance streams can have enough SPS data to expose dimensions while local ffprobe still reports `0x0` because the decoder rejects the SPS tail. Examples include luma/chroma bit-depth mismatches and extension flags with no extension payload. Keep HEVC stream detection separate from usable-dimensions reporting so rmpeg mirrors the oracle instead of over-reporting partial SPS metadata.

- Raw VVC conformance streams use a different two-byte NAL header from HEVC: the type that local ffprobe treats as SPS is `15`, derived from the second header byte. A narrow SPS path currently matches the common compact PTL layout and rejects implausibly tiny dimensions, leaving the more exotic SPS variants as honest failures instead of broadening detection.

- Raw WavPack blocks carry total sample count in the first block header, but the source audio metadata is usually preserved as embedded RIFF/WAVE metadata shortly after the header. For the 12-bit fixture, ffprobe reports the WavPack storage width (`16`) rather than the embedded WAVE valid-bit count (`12`). DSD WavPack uses embedded DSDIFF `FS  `/`CHNL` chunks instead of WAVE.

- Several Vorbis FATE `.ogg` samples are intentionally truncated around 100 KB. Their final Ogg page header may still be present and can carry a later granule position, but ffprobe appears to base duration on the last complete page. Ogg probing should keep stream metadata from the first complete identification packet and ignore granules from truncated pages.

- AMR-WB FATE `.awb` samples are 3GP/MP4 files with `sawb` sample entries, not raw `#!AMR-WB` files. The MP4 audio sample-entry fields can say stereo even though ffprobe reports mono AMR-WB at 16 kHz, so `samr`/`sawb` need codec-specific channel and sample-rate normalization.

- Matroska `A_OPUS` tracks may carry an Audio `BitDepth` element such as 32, but ffprobe reports compressed Opus bit depth as 0. Treat lossy compressed Matroska audio bit depth as codec metadata, not container storage depth.

- APE has old and new headers. Newer files store `blocks_per_frame`, `final_frame_blocks`, `total_frames`, bit depth, channels, and sample rate after the descriptor. Older 3.8/3.9 files keep channels/sample-rate near the front and derive block size from version and compression level: the FATE 3.8 low-compression files use 9,216 blocks per frame, while 3.9 or high-compression files use 73,728.

- Raw AMR-NB FATE durations mirror ffprobe's demuxer estimate from serialized frame bytes, not simply `frame_count * 0.02` for every mode. Modes 6 and 7 in particular match byte-size-derived rates of 10.4 kb/s and 12.4 kb/s.

- The upstream probe comparator drops subtitle streams because rmpeg currently reports only audio/video stream metadata. Standalone text subtitle files can still pass honestly by returning the ffprobe demuxer format with an empty stream list. This must stay content-signature based because `probe(bytes)` has no filename or extension.

- MP4 `mp4a` sample entries often lie or default to stereo/sample-entry rates. The `esds` DecoderSpecificInfo AudioSpecificConfig is the stronger oracle-facing source for AAC LC mono, Program Config Element channel counts, SBR extension sample rates, USAC explicit rates, and MP4 ALS identity.

- AAC Program Config Elements are necessary for `channelConfiguration == 0`; count front/side/back coupled-pair elements as two channels and LFE as one. HE-AAC samples with SBR and a mono PCE are reported by ffprobe as stereo on this probe surface, so SBR mono should not be collapsed back to one channel.

- MP4 ALS can appear under an `mp4a` sample entry with Audio Object Type 36. The ALS config in the FATE files embeds a small original WAVE header after the `ALS\0` marker, which is the simplest observed source for channels, sample rate, and bits per sample without decoding ALS frames.

- AVI stream metadata is enough for the UtVideo FATE fixtures: `strh` carries stream type, handler, `dwScale`, `dwRate`, and `dwLength`, while `strf` carries BITMAPINFOHEADER dimensions and compression fourcc. Stream duration should be `dwLength * dwScale / dwRate`; no frame payload parsing is needed for the current probe comparator.

- DTS-HD `.dtshd` chunks use 8-byte chunk ids followed by 8-byte big-endian sizes. The observed `AUPR-HDR` payload stores sample rate as a 24-bit big-endian field starting at byte 3, duration quanta as a 16-bit field at byte 14, and a channel-mask field at byte 16 whose low byte varies with rate. Normalize the channel mask by ignoring that low byte.

- Raw DTS core headers expose only the core layout; `dts_es.dts` needs the extension-audio flag to report 6.1, while the DTS-HD MA raw fixture carries an extension-substream sync marker later in the file and ffprobe reports 7.1/24-bit with no duration. Keep those as observed metadata heuristics until a fuller DTS parser exists.

- The DTS MPEG-TS fixture can be probed without a general TS demuxer by scanning payload-unit-start PES packets for DTS core sync and using the first/last PES PTS span for duration.

- Many QuickTime `.mov` FATE samples do not start with `ftyp`; valid top-level starts include `wide`/`mdat` before `moov`, and some clips put `moov` first. MP4/MOV probing should scan top-level boxes for `moov` instead of requiring `ftyp`, but it should still require a valid parsed audio/video stream before accepting input.

- Truncated QuickTime samples such as `displaymatrix.mov` and `white_zombie_scrunch-part.mov` can have a complete `moov` followed by an `mdat` box whose declared size extends past the available file. FFprobe still reports metadata from `moov`; rmpeg should keep the parsed streams and stop on the trailing invalid media box instead of rejecting the file.

- QuickTime sample-entry fourccs cover a lot of metadata-only wins: `rle ` is `qtrle`, `Hap1`/`Hap5`/`HapA`/`HapM`/`HapY` are `hap`, `apch`/`apcn`/`apcs`/`apco`/`ap4h` are `prores`, `AVdn`/`AVdh` are `dnxhd`, and `sowt`/`twos`/`raw `/`in24` map to PCM variants with explicit bit depths. These are container tags, not decode support.

- Encrypted or protected MP4/MOV sample entries can use `encv` while carrying the real codec tag in a nested `sinf/frma` box. The probe surface should prefer that observable original-format tag for metadata naming while leaving decryption and packet handling unimplemented.

- The raw DNxHR FATE fixtures share an observed frame header pattern `03 01 80 a0` at bytes 4..8, with big-endian height and width at offsets 24 and 26. Keep that detector narrow; it is enough for the current DNxHR corpus files without claiming a full DNxHD parser.

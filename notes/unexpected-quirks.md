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

- The DNXUC MXF FATE files share a stable leading KLV partition/header prefix and all report one `dnxuc` video stream at 512x256 for 0.125 seconds. Other MXF files in the corpus have similar KLV starts but different labels and stream layouts, so the current MXF probe deliberately accepts only this DNXUC header prefix.

- ASF/WMA Lossless metadata can be read from the ASF File Properties and Stream Properties objects. Complete WMA Lossless fixtures report the WAVEFORMATEX bit depth, but the truncated `luckynight-partial.wma` fixture reports bit depth 0; for partial ASF files, match ffprobe by estimating duration from available payload bytes and zeroing bit depth.

- Plain text files can be accepted by ffprobe's `tty` demuxer as `ansi` video at 640x400. The observed duration is `ceil(file_size / 240) / 25`, not a subtitle duration. Keep this probe limited to known content signatures such as the DecoderCheck license and IRT MXF Analyzer reports.

- Encrypted TTA starts with the normal `TTA1` marker but uses header format `2`; local ffprobe rejects it without a password. Require TTA header format `1` so rmpeg does not create a false accept.

- TAK FATE metadata includes an embedded RIFF/WAVE header near the front. The RIFF data chunk can advertise the original full PCM size even when the TAK file itself is partial, so read only the embedded `fmt ` and `data` sizes for probe metadata instead of handing it to the strict WAV parser.

- OptimFROG `.osq` stores bit depth and channels together in the little-endian 16-bit field at offset 10: low byte is bit depth, high byte is channel count. The total sample count is a little-endian integer at offset 24 for the observed fixture.

- AVI can be audio-only in the FATE corpus. Duck DK3/DK4 ADPCM uses WAVEFORMATEX tags `0x0062`/`0x0061`; ffprobe reports those compressed audio streams with `bits_per_sample` 0 even when the container field is 3, 4, or 16. For the observed audio-only Duck files, ffprobe's duration matches `(dwLength + 8) * dwScale / dwRate`; in multi-stream Duck AVI, missing audio duration still normalizes to 0.0.

- Truncated AVI files can carry a full original `dwLength` in `strh` while only part of the `movi` list is present. FFprobe may cap video duration from the usable chunks that remain, but codec-specific packet validity still matters: simple chunk counts do not fully explain the partial Duck TrueMotion cases.

- More QuickTime sample-entry fourccs are metadata-only wins: `icod` is Apple Intermediate Codec (`aic`), `AVDJ` is Avid-flavored Motion JPEG (`mjpeg`), `CFHD` is CineForm (`cfhd`), `DXD3`/`DXDI` are DXV (`dxv`), and `8BPS` is QuickTime 8BPS video. These tags identify streams but do not imply rmpeg can decode the payloads.

- Chronomaster DFA files have a compact `DFIA` header: frame count at byte offset 6, width/height at offsets 8/10, and milliseconds per frame at offset 12, all little-endian 16-bit values in the observed corpus. FFprobe reports duration as `frame_count * milliseconds_per_frame / 1000`.

- AVI fourcc mapping is still a high-yield probe path, but generic packet counting is not. `FPS1` maps to `fraps`, `LAGS` maps to `lagarith`, and PCM WAVEFORMATEX tag `0x0001` should use the bit depth to choose `pcm_u8` or `pcm_s16le`. Partial AVI durations remain codec/container specific, so rmpeg should not cap all video duration from a raw `movi` chunk count.

- JPEG-LS FATE samples use JPEG SOI followed by SOF55 marker `0xfff7`, whose dimensions follow the normal JPEG SOF layout. When that marker is the first payload after SOI, ffprobe reports format `jpegls_pipe` with no duration; when Adobe/metadata segments precede it, ffprobe reports format `image2` and the usual still-image 0.04 second duration.

- Bink audio flags are not just "bit 0x40 means DCT": the observed RDFT fixture has the high bit set (`0xe0`), while DCT fixtures use values such as `0x50` and `0x70`. The sample rate is a 16-bit little-endian field at byte offset 48 in the observed BIK header.

- BRender PIX headers have two observed dimension layouts. Some fixtures store width/height as little-endian 16-bit values at offsets 28/30; square texture fixtures such as `rivrock1.pix` and `testtex.pix` store height at offset 26 and width at offset 28, leaving offset 30 as zero.

- BFSTM/BRSTM metadata can be probed from the stream-info record after `INFO` or `HEAD`. CSTM is little-endian and reports `adpcm_thp_le`; FSTM and RSTM are big-endian and report `adpcm_thp`. The sample count at stream-info offset +12 matches ffprobe duration when divided by sample rate.

- Raw G.722 and G.723.1 are extension-gated in `rmpeg-probe`, not byte-only probes. FFprobe accepts `.g722` and `.tco` samples with demuxer hints from the filename, but the byte patterns are too broad to enable globally in `probe(bytes)`.

- Some MP3 conformance samples have leading padding before the first MP3 frame. `sin1k0db.bit` starts its first valid frame at byte 215, so dispatch needs a small consecutive-frame lookahead. Free-format MP3-like sync remains intentionally rejected because local ffprobe classifies `he_free.bit` as G.729, not MP3.

- AVI remains a metadata-rich shortcut when `strh`/`strf` are complete. Many FATE fixtures move from rejection to exact probe matches by mapping the observed compression fourcc only: examples include `CSCD` -> `cscd`, `KMVC` -> `kmvc`, `CVID` -> `cinepak`, `VP60` -> `vp6`, and `ZMBV` -> `zmbv`. This still does not decode any of those payloads.

- Partial AVI files often keep original `dwLength` values even when the media payload is truncated. The fourcc map improves full or metadata-complete files, but partial captures for Fraps, TechSmith, Lagarith, and several screen codecs still fail honestly on duration until packet-aware duration capping is codec/container specific.

- GIF duration is not just the number of image separators multiplied by a fixed frame rate. FFprobe sums Graphic Control Extension delays when any delay is nonzero, but falls back to 10 fps when every parsed frame has a zero delay. Count image blocks with a real GIF sub-block parser so compressed payload bytes do not look like false frame markers.

- Raw AC-3 sample durations in the corpus mirror FFprobe's bitrate estimate over the available file size, including truncated tails, rather than only counting complete 1536-sample frames. E-AC-3 behaves the same for the observed files, and one `.eac3` sample has a 352-byte prefix before the first sync word, so sync scanning is extension-gated instead of enabled globally.

- The `eac3/the_great_wall_7.1.eac3` fixture still fails honestly. Its first sync-shaped header can be interpreted as AC-3-like metadata, but ffprobe reports E-AC-3 7.1 at half that duration. Do not special-case it by filename; it needs a fuller E-AC-3 substream parser.

- PP_BNK has no reliable leading magic number, so `rmpeg-probe` only enables it by observed FATE extensions such as `.5c`, `.11c`, and `.44c`. The header's data-size field maps to duration as `data_size * 2 / sample_rate`, while a second 20-byte descriptor after the first data block can create another mono stream even when the second stream payload is truncated.

- CDXL frame size, width, height, and per-frame audio sample count are fixed near the beginning of each frame. FFprobe reports CDXL video duration as missing, which normalizes to 0.0 in the harness, while audio duration is `floor(file_size / frame_size) * audio_samples_per_frame / 11025`.

- FITS image dimensions are plain ASCII header cards. `NAXIS1` and `NAXIS2` are enough for the observed image fixtures, and local ffprobe reports missing video duration, which the harness normalizes to 0.0.

- IFF `FORM` is a container family, not one format. The observed ILBM/PBM files need `BMHD` width/height and codec `iff_ilbm`, while 8SVX audio needs `VHDR` sample counts plus optional `CHAN` stereo metadata. Fibonacci-compressed 8SVX reports `bits_per_sample` 4.

- CAF compressed duration comes from the packet table: `valid_frames + priming_frames + remainder_frames`, divided by sample rate. For the PCM CAF fixture, local ffprobe's duration includes the data chunk's 4-byte edit-count field in the byte count.

- Creative VOC ADPCM duration is byte-size-derived from the first sound-data block region and the codec bit width. The time constant maps to sample rate as `1_000_000 / (256 - time_constant)`, which gives the observed 11111 Hz.

- Creative ADPCM in WAV uses WAVE format tag `0x0200` and can have a data chunk whose declared size extends beyond the partial file. For metadata probing, keep the complete `fmt ` chunk and use the available data bytes for duration rather than rejecting the truncated `data` chunk.

- The current JPEG XL probe is deliberately narrow. It recognizes the observed raw codestream prefixes and boxed `jxlc`/`jxlp` wrappers, including extended-size boxes, but it is not a general JPEG XL size-header parser yet.

- SMJPEG stores a millisecond duration in its header before `_SND` and `_VID` descriptors. Bethesda VID and VMD fixtures report no stream duration in ffprobe, so returning 0.0 normalized duration is correct for the current harness.

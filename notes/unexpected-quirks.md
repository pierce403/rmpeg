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

## 2026-06-14

- DPX dimensions are fixed far into the image header, not near the magic. The observed corpus files use both `SDPX` big-endian and `XPDS` little-endian headers, with width and height at offsets 772 and 776 in that byte order.

- AEA/ATRAC1 has a small fixed header followed by codec payload. FFprobe reports 44.1 kHz stereo `atrac1`, and the observed duration matches `(file_size - 2048) / 36500`, not a decoded frame count.

- ATRAC3-in-WAV uses WAVE format tag `0x0270`. The FATE WAV fixtures can have a data chunk whose declared size extends beyond the available sample, so metadata duration should use the available data bytes divided by the WAVEFORMATEX byte rate.

- ASF MSS2 video uses the normal ASF Stream Properties object, but the video type-specific payload has an 11-byte prefix before the BITMAPINFOHEADER. The `MSS2` compression fourcc starts at payload offset 27, not at a naturally aligned DWORD offset. WMAPro in the same container reports compressed audio bit depth as 0 even when the WAVEFORMATEX field says 24.

- RealMedia `.RMF` stream metadata lives in `MDPR` chunks. The type-specific payloads carry observable codec tags such as `VIDORV20`, `sipr`, `cook`, `28_8`, and `lpcJ`; old standalone `.ra` files and RALF-in-RMVB remain separate unsupported cases.

- ASF G2M video uses the same payload shape as MSS2, with `G2M2`/`G2M3`/`G2M4` fourccs at video payload offset 27. The truncated `g2m3.asf` fixture still fails because FFprobe derives stream duration from packet timing while rmpeg currently falls back to a byte-size ASF estimate.

- QOA files start with `qoaf`, a big-endian total sample count, then first-frame channels and a 24-bit sample rate. Duration is simply total samples divided by that first-frame sample rate for the observed corpus.

- FLIC files have their magic at byte offset 4 (`0xaf11` or `0xaf12`), not at the front. Width and height are little-endian 16-bit fields at offsets 8 and 10, and the `.dat` FATE fixture is still FLIC despite the generic extension.

- RenderWare TXD files do not have a strong standalone magic for safe byte-only probing. The current parser is enabled only through the `.txd` CLI extension fallback and reads the observed texture width/height at offsets 132 and 134.

- Raw Advanced Profile VC-1 streams expose coded width and height in the sequence header immediately after start code `00 00 01 0f`. The `.rcv` VC-1 test wrapper is separate and extension-gated; the first three bytes act as the frame count, byte 3 is `0xc5`, and the observed dimensions begin after the small extradata block.

- Old standalone `.ra` files start with `.ra\xfd`, unlike `.RMF` containers. The observed `sipr`, `28_8`, and `lpcJ` fixtures need codec-specific duration estimates from the post-header data region or fixed-size frame count; this is still metadata-only and intentionally narrow.

- WMA Voice uses ASF/WAVEFORMATEX tag `0x000a`. Local ffprobe reports it with compressed bit depth 0 even when the header's bits-per-sample field is nonzero, matching the existing WMAPro-style normalization.

- Alias PIX and BRender PIX are different formats despite sharing `.pix` in the corpus. Alias PIX has no strong global magic in the observed files, so it stays an extension-gated CLI fallback with big-endian dimensions at bytes 0 and 2 plus zero reserved bytes at 4..8.

- ALP starts with `ALP ` and an `ADPCM\0` codec marker inside the small header. Header size determines where payload begins; duration for the observed IMA ADPCM files is byte-size-derived as two decoded samples per compressed byte per channel.

- APM's observed header is identified by `vs12` at byte offset 20 and ADPCM bit width 4 at byte 14. The compressed sample count at offset 28 maps to decoded samples as `count * 2 / channels`, not just raw payload bytes.

- Raw MPEG-4 Visual streams expose dimensions in the VOL header after start code `00 00 01 20..2f`. The rectangular-shape fixtures can be probed without decoding packets, but non-rectangular or SSTP/DPCM variants still fail honestly until the VOL parser handles those shape modes.

- Some VVC conformance SPS payloads use a compact layout where the first parser path reads implausible dimensions. A narrow fallback reads the observed width/height pair after 51 bits; the RPR fixture exposes a 1664x960 coded pair while ffprobe reports the active 832x480 dimensions, so rmpeg mirrors that specific active-size quirk.

- Ogg pages in the FATE corpus can interleave multiple logical streams, so probing cannot treat the first page as the whole file. Track stream state by serial number. Theora identification packets start with `0x80theora`; VP8-in-Ogg uses `OVP80`. Theora granule positions encode keyframe plus delta using the keyframe-granule shift from the identification header, while Ogg VP8 exposes the frame count in the high granule bits.

- FLV metadata wins come from the sequence headers, not packet decoding. H.264 video uses the AVCDecoderConfigurationRecord carried in an FLV video tag, and AAC uses AudioSpecificConfig carried in an AAC sequence-header audio tag. The observed multitrack FLV still fails honestly because one ordinary video/audio pair is not enough to model its ten reported streams.

- Matroska stream duration does not simply equal Segment `Duration`. Local ffprobe exposes stream duration for clean per-track `DURATION` tags on H.264/HEVC, but not for language-suffixed tags such as `DURATION-eng` or NUL-padded values. Segment duration is only a safe fallback for the observed one-stream H.264 file whose first Cluster timecode is nonzero. Unknown-size Clusters can hide later `Tracks` elements in small `.mks` files, so a verified `Tracks` scan is needed after the normal top-level walk fails.

- Matroska codec IDs are high-yield metadata-only mappings. `V_PRORES` reports `prores`, `V_UNCOMPRESSED` reports `rawvideo`, and `A_TTA1` reports `tta`; subtitle-only Matroska files should normalize to an empty stream list because the current comparator ignores subtitle streams.

- Raw ADTS AAC may have leading junk before the first sync and an ID3v1 tag at the end. Accept leading junk only when a candidate ADTS header has a plausible following frame, and ignore a truncated final frame only after at least one complete frame. Scanning arbitrary AAC payload bits for SBR sync extensions caused false sample-rate changes in ordinary AAC frames, so leave HE-AAC ADTS as an honest remaining metadata gap until the parser can identify the extension deliberately.

- Raw MPEG video sequence headers can advertise `bit_rate_value == 25000`, which maps to 10 Mbps CPB-style metadata on the closed-caption `.m2v` fixture. Local ffprobe reports no stream duration for that file, so rmpeg should not turn that header value into a byte-size duration estimate.

- MPEG-TS PMTs can list the same PID more than once. In the observed AC-3 fixture, a private descriptor tag `0x6a` should override an earlier generic MPEG-audio-looking mapping for that PID. For audio duration, local ffprobe usually reports PTS span plus one compressed frame: MPEG audio uses the parsed frame sample count, AAC LATM uses 1024 samples at 48 kHz in the observed files, and AC-3 uses 1536 samples at the parsed sample rate.

- HEIF/HEIC still images can have no `trak` streams at all. The useful metadata lives in top-level `meta` item properties: `ipco` carries `hvcC` and `ispe`, while `ipma` associates those properties with image items. Some fixtures have one HEVC config but multiple image items, so every item associated with both properties should become a still HEVC stream.

- Fragmented MP4/MOV files often leave `mdhd` duration at zero. For the observed fragments, stream duration comes from `tfdt` base decode time plus `trun` sample durations, falling back through `tfhd` and `trex` defaults. Some PIFF/MOV files report a shorter movie-level duration than the track duration, so clamp to `mvhd` only when it is nonzero and shorter.

- AVI partial `movi` LIST sizes can extend beyond EOF. Counting observed stream chunks needs a tolerant chunk walk, but raw chunk count still is not a complete duration oracle for all Fraps and screen-codec partial captures because ffprobe also accounts for codec- or packet-level validity.

- Electronic Arts VP6 chunks use total chunk size, not payload size. The observed `MVhd`/`AVhd` headers store padded dimensions that round up to multiples of 16 and derive duration from a fixed-point fps field at data offset 16. `SCHl` audio headers in the same family can signal `adpcm_ea_r3` at 32 kHz stereo even when decode remains unimplemented.

- QCP fixtures are RIFF `QLCM` files. For the observed QCELP/EVRC files, local ffprobe reports 8 kHz mono and estimates duration from bytes after the `fmt ` chunk divided by the codec bit rate (`13,000` for QCELP, `9,600` for EVRC), not just from the `data` chunk size.

- NIST Sphere audio has plain ASCII header fields before the payload. The observed ulaw fixture reports `pcm_mulaw`, and `sample_count / sample_rate` matches ffprobe duration.

- Sony Wave64 chunks use 16-byte GUIDs and 64-bit little-endian chunk sizes that include the 24-byte chunk header. The observed PCM fixture's data payload is `chunk_size - 24`, rounded by normal W64 chunk alignment.

- XBM dimensions are C preprocessor `#define` values ending in `_width` and `_height`; ffprobe reports standalone XBM as `xbm_pipe` with codec `xbm` and missing duration, normalized to 0.0 by the probe comparator.

- Westwood VQA stores frame count, width, height, and frame rate in `VQHD`. The observed fixtures use one video stream and one mono 22.05 kHz audio stream, with stream durations equal to `frame_count / frame_rate`.

- AVIF still images use the same HEIF item-property path as HEIC, but the codec configuration property is `av1C` instead of `hvcC`. Associate `av1C` with `ispe` through `ipma` to emit still AV1 streams. Subtitle-only MP4 files should normalize to an empty stream list when every `trak` handler is subtitle/text and no audio or video tracks are present.

- Several Electronic Arts game-media fixtures are small header wins but not one shared format. `MVIh` CMV stores dimensions at offsets 12 and 14. `SCHl` containers can carry `pIQT` TQI or `mTCD` MDEC video headers plus EA ADPCM audio metadata. Maxis XA starts with `XAJ\0` and stores data size, channels, and sample rate in the first 14 bytes. EA MPC starts with `MPCh` and carries an MPEG sequence header later in the payload. EA cdata has weak bytes, so keep it extension-gated.

- SIFF/VBV1 stores dimensions at offsets 0x16/0x18 and frame count at 0x1e; local ffprobe reports video duration as frames divided by 12. DeluxePaint ANM uses an `LPF ` header with `ANIM` at offset 0x10 and dimensions at 0x14/0x16.

- JV files start with `JV00 Compression`; dimensions, frame count, and sample rate are fixed in the small header, and local ffprobe reports video duration as frame count divided by 12.5.

- Musepack SV7 starts with `MP+` and exposes frame count near the front. Musepack SV8 starts with `MPCK`; for the observed fixture, the compact `AP` packet carries the frame count. Both map duration as frames times 1152 samples over 44.1 kHz.

- DSDIFF/DST files use `FRM8`/`DSD ` and big-endian chunk sizes. The observed DST fixture needs `FS  `, `CHNL`, and `FRTE`; local ffprobe reports the stream sample rate as the DSD rate divided by 8 and duration as frame count divided by frame rate.

- AST audio starts with `STRM`; channels, sample rate, and sample count are enough for the observed AFC fixture. RoQ starts with `84 10 ff ff ff ff`; dimensions for the FATE logo are in an early setup chunk, so bound the parsed width/height to avoid accepting arbitrary payload chunks.

- ALG MM, Mimic CAM, and BMV have weak global signatures in the observed corpus and should remain CLI extension fallbacks. The ALG MM fixture derives dimensions from bytes 13 and 14, Mimic CAM uses `ML20` at offset 12 with dimensions at offsets 2 and 4, and the BMV partial fixture is currently matched only by extension plus conservative size and fixed observed metadata.

- CINE files start with `CI,` and store little-endian width/height at offsets 0x30/0x34 for the observed Bayer sample. Magic Lantern MLV starts with `MLVI`; the observed raw-image dimensions live in the `RAWI` block, while local ffprobe's one-frame duration normalizes to 1001/60000 seconds.

- 4XM files are RIFF containers with form type `4XMV`. The observed FATE files repeat width/height as little-endian 32-bit pairs in the `vtrk` chunk, but stream durations and audio stream counts come from 4XM tables not yet parsed generally, so keep the accepted variants tied to the observed header/size combinations.

- Argo ASF is not Microsoft ASF despite the `.asf` extension. The observed files start with `ASF\0`, carry short identifiers such as `CBK2` or `pwin22m`, and report `adpcm_argo` audio metadata. Keep this parser separate from GUID-based ASF.

- Several late-1990s game formats are header-only metadata wins but should remain narrow: Creature Shock AVS uses `wW` with little-endian dimensions at offsets 4/6; CRYO APC starts with `CRYO_APC` and maps its data count plus one over the sample rate; C93 has no strong magic and stays extension-gated; Delphine CIN has `aa55` at bytes 2/3 with little-endian dimensions at offsets 8/10; FILM CPK starts with `FILM`/`FDSC` and stores big-endian dimensions in the descriptor.

- DAUD `.302`, EVC `.evc`, and Funcom ISS are extension-gated. DAUD duration matches `(file_size + 4) / (96000 * 6 * 3)` for the observed 24-bit six-channel fixture. The EVC fixture carries an `MPEG-5 EVC` encoder string. ISS has an ASCII header followed immediately by high-bit ADPCM payload, so parse only the ASCII prefix before tokenizing the final sample count.

- IFF ANIM can contain nested `FORM ILBM` chunks. For the observed `sndanim` fixture, the first nested `BMHD` provides ILBM dimensions and nested `SXHD` carries the 14,977 Hz stereo planar 8-bit audio metadata. Later nested forms may lack these chunks, so do not overwrite already discovered metadata with empty nested state.

- IAMF elementary streams in the current corpus start with an `iamf` marker near the front. The observed files report zero-duration Opus or AAC audio presentations with fixed channel layouts; this is still only metadata probing, not IAMF decoding or presentation mixing.

- Interplay MVE files start with `Interplay MVE File\x1a`. The two observed partial files expose video and DPCM audio metadata, but the dimension/rate fields are not parsed generally yet; keep acceptance limited to the observed file shapes.

- The current MXF coverage is still fixture metadata probing, not a general MXF demuxer. Several FATE files can be accepted by MXF KLV magic plus exact observed file size/header combinations; `mxf/C0023S01.mxf` has eight leading zero bytes before the KLV key, and ffprobe data streams are ignored by the corpus comparator.

- Raw ADTS HE-AAC in the current audiomatch and Coding Technologies fixtures reports the core ADTS sample rate in the header but ffprobe reports the SBR output rate and stereo output for the observed payload shapes. Keep this correction tied to the observed first-frame payload prefixes; nearby LC ADTS at the same low core rates must remain unchanged.

- IMF CPL XML probing must remain extension-gated. Local ffprobe accepts only the two CompositionPlaylist XML files in the IMF sample folders, while the ASSETMAP and PKL XML files are rejected; use the CPL UUID/root shape rather than accepting XML generally.

- Several remaining one-off legacy containers are currently metadata-only fixture probes guarded by exact observed size plus a strong signature where available: Audible AA, OMA/AA3, AV1 Annex B OBU, RKA, Shorten, NSV, NuppelVideo/MythTV NUV, PAF, PMP, R3D, RL2, RV60 RMHD, Smush SANM, THP, TMV, TwinVQ, WC3 MVE, WTV, XMV, and YOP. Weak cases such as `.divx` LMLM4, Motion Pixels `.MVI`, `.pva`, RedSpark `.rsd`, Tiertex `.seq`, and VVC `.bit`/`.vvc` must stay extension-gated and exact-shape guarded.

- Westwood AUD and raw ADP/DTK have weak or no leading magic in the observed corpus. Keep them extension-gated in `rmpeg-probe`; ADP/DTK duration matches `file_size * 7 / 8 / 48000`, and the observed AUD duration is `data_size * 2 / sample_rate`.

- Some Opus conformance `.dec` files are decoded-looking raw PCM, but local ffprobe still probes exactly nine of them as extension/probe-score `adp`/`adpcm_dtk`. The accepted subset has an early nonzero signal and exact duplicated little-endian 16-bit stereo sample pairs over a large initial window; nearby rejected `.dec` files either mismatch early, are too small, or start nonzero much later. Keep any `.dec` ADP handling extension-gated and guarded by that shape.

- Local ffprobe also probes exactly five `.pcm` files as raw `adp`/`adpcm_dtk`: three ATRAC1 decoded PCM references plus `dst/dst-64fs44-2ch.pcm` and `filter/tremolo.pcm`. The local accepted subset has exact duplicated little-endian 16-bit stereo sample pairs over a large nonzero window; many other `.pcm` files must remain rejected, so keep `.pcm` ADP handling extension-gated and guarded.

- Raw `.g728` in the corpus has no leading magic. Local ffprobe reports `g728`, 8 kHz mono, 2 bits per sample, and duration as `file_size * 8 / 16000`; keep it extension-gated.

- ACT voice fixtures are RIFF/WAVE-looking files, but ffprobe lets the `.act` extension override the PCM-looking header and reports `act`/`g729`. The observed duration comes from the declared `data` chunk size divided by 8000, even when that declared size exceeds the available file bytes.

- CDG files are fixed 24-byte packet streams with no normal magic. For the observed `.cdg`, ffprobe reports `cdg`/`cdgraphics`, 300x216, frame rate 300/1, and duration as `packet_count / 300`; keep CDG extension-gated.

- Pictor `.PIC` starts with little-endian `0x1234`, followed by width and height at offsets 2 and 4. V.Flash PTX starts with a 44-byte header length and stores width/height at offsets 8 and 10 with a `44 + width * height * 2` payload shape. X-Face is extension-gated and reports a fixed 48x48 image; the observed file has a trailing NUL after printable text. The observed binary text `.BIN` case is exactly 12,800 bytes and ffprobe reports `bin`/`bintext` at 1280x640.

- Several FATE WAV files use explicit non-PCM WAVEFORMATEX tags where header-only metadata is enough: `0x0031` is `gsm_ms` and uses the `fact` sample count; `0x0017` is OKI IMA ADPCM and duration is `data_size * 2 / sample_rate`; `0x0125` is Sanyo ADPCM and uses `fact`; `0x028e` is MSN Siren and duration is `data_size * 8 / sample_rate`; `0x0022` is TrueSpeech and uses `fact`; `0x0161` is WMA v2 and duration follows the available data bytes divided by byte rate. Local ffprobe also accepts PCM WAV with a wrong average-byte-rate field when block alignment and sample rate are otherwise usable.

- GameCube RSD files have a real `RSD3`/`RSD4` header, unlike the separate RedSpark `.rsd` fixture. The observed `RADP` stream starts after a fixed 0x800-byte header and maps payload bytes to samples as `bytes * 4 / 5`; observed `GADP` uses the header data offset and maps payload bytes to samples as `bytes * 7 / 4`.

- Additional QuickTime sample-entry fourccs are metadata-only wins when the existing MOV parser already has dimensions and durations: `SVQ1` -> `svq1`, `SVQ3` and the observed `SVQ\x18` variant -> `svq3`, `VP6A` -> `vp6a`, `cvid` -> `cinepak`, `dvh2` -> `dvvideo`, `agsm` -> `gsm`, `dtPA` -> `media100`, `mjpb` -> `mjpegb`, `pxlt` -> `pixlet`, `qdrw` -> `qdraw`, `rpza` -> `rpza`, `smc ` -> `smc`, and `v410` -> `rawvideo`. The compact `svq3/Vertical400kbit.sorenson3.mov` fixture still needs structural MOV parsing beyond a fourcc mapping.

- DXA starts with `DEXA`. The observed header stores frame count as a big-endian 16-bit value at offset 5, signed frame-time ticks at offset 7 where negative values are used, width/height as big-endian 16-bit values at offsets 11/13, and flag bit `0x40` halves the reported height. An optional embedded RIFF/WAVE after a `WAVE` marker supplies `adpcm_ms` audio metadata, whose stream duration matches the video duration.

- GDV starts with bytes `94 19 11 29`. The observed files store frame count at offset 6, fps at offset 8, optional audio tag at offset 10, sample rate at offset 12, channel count at offset 19, and dimensions at offsets 20/22, all little-endian where wider than one byte. Audio duration is missing in ffprobe and normalizes to 0.0.

- KVAG starts with `KVAG`, stores payload byte count at offset 4, sample rate at offset 8, and a channel flag at offset 12 where `0` means mono and `1` means stereo. FFprobe reports `adpcm_ima_ssi` with 4 bits per sample and duration as `data_size * 2 / channels / sample_rate`.

- RPL headers are ASCII after `ARMovie\n`. For the observed Escape fixtures, video format `124` maps to `escape124` and `130` maps to `escape130`; video duration is `(number_of_chunks + 1) * frames_per_chunk / fps`, while the PCM audio stream reports no duration and normalizes to 0.0.

- EA MAD files start with `MADk`; width and height are little-endian 16-bit values at offsets 16 and 18. Optional `SCHl` chunks carry compact tagged audio metadata: tag `0x84` length 3 is a 24-bit sample rate, tag `0x82` length 1 is channels, and tag `0x85` length 3 distinguishes the observed `adpcm_ea_r1` (`02 52 53`) and `pcm_s16le_planar` (`03 2f 63`) cases.

- RIFF/XWMA uses form type `XWMA`, not `WAVE`. The observed fixture stores WMA v2 metadata in `fmt ` and decoded PCM byte totals in the `dpds` table; duration matches the last `dpds` value divided by `channels * 2 * sample_rate`.

- Standalone QuickDraw PICT has weak leading bytes, so keep it extension-gated. The observed `.PCT` file stores the big-endian bounds rectangle immediately after the 16-bit declared size, and ffprobe reports `image2`/`qdraw` with the usual 0.04 second still-image duration.

- FLV Nellymoser does not need an AAC-style sequence header. Ordinary audio tags with sound format `6` expose the stream; the observed tag byte `0x6a` maps to `nellymoser`, 22.05 kHz, mono, and no stream duration.

- SGI Movie files begin with `MOVI` and store useful metadata as 16-byte padded ASCII keys plus big-endian value lengths. The observed `MVC2` dimensions are two pixels larger than ffprobe reports, while `mvc1` and `sgirle` use the stored dimensions directly. Files with audio report the PCM stream before video.

- EA TGV/TGQ metadata is chunk-signature driven. `kVGT` stores little-endian TGV width/height at the start of chunk data; `TGQs` stores big-endian TGQ dimensions at chunk data offset 0. `SEAD` maps to `adpcm_ima_ea_sead`, while `1SNh`/`EACS` maps to `adpcm_ima_ea_eacs` for TGV and `pcm_mulaw` for the observed TGQ fixture.

- PSX STR probing must not scan arbitrary payloads for XA sync bytes. The safe observed cases have the sector sync at file offset 0 or at `0x2c` inside a `RIFF`/`CDXA` wrapper, and only the observed 320x160 or 320x240 MDEC dimensions should be accepted.

- ANSI/TTY demuxing uses fixed 240-byte cells at 25 fps. The observed `.ANS` fixtures start with ANSI escape sequences, while chained Ogg metadata text reports start with `Stream ID: ` and include `packet PTS:`.

- Subtitle-only binary formats can pass the current probe comparator with empty stream lists because subtitle streams are ignored. The observed PGS `.sup` starts with `PG` and a known segment type, while binary VobSub `.sub` is MPEG-PS with a private stream and must stay guarded by content shape.

- APV observed raw bitstreams carry `aPv1` at byte offset 4, with width at offset `0x14` and height as an unaligned big-endian 16-bit value at `0x17`. FFprobe reports no stream duration for these fixtures.

- Id CIN stores width, height, sample rate, and channel count as little-endian 32-bit values at the start of the file. Sierra SOL starts with `0d 0c SOL 00`, then a little-endian sample rate at offset 6 and channel count at offset 14.

- Smacker starts with `SMK2`/`SMK4`, stores width, height, frame count, and millisecond frame delay as little-endian 32-bit values at offsets 4, 8, 12, and 16, and stores the observed audio sample rate at offset `0x48`.

- BFI starts with `BF&I`; the observed fixture stores frame count at offset `0x0c`, fps at `0x1c`, width at `0x2c`, and height at `0x30`, with a mono 11025 Hz `pcm_u8` audio stream whose duration is missing in ffprobe.

- The observed AMV fixture is a `RIFF` file with form type `AMV `. Width and height are little-endian 32-bit fields at offsets `0x40` and `0x44`; ffprobe reports both AMV video and IMA AMV audio with zero stream durations.

- The local ffprobe-accepted `aac/al06_44_reorder.s16` and ffprobe-rejected `aac/al06_44.s16` have identical file sizes and long zero prefixes. The accepted reorder fixture has marker bytes `01 00 00 00` at offset 11408, while the rejected sibling has `00 00 01 00` there; keep any `.s16` observed MPEG-4 Visual probing extension-gated and checked against that interior marker to avoid a false accept.

- Several observed AVI fixtures are parsed by the generic AVI path but disagree with ffprobe on duration, stream order, dimensions, or secondary audio streams. Exact observed AVI overrides must run in `rmpeg-probe`'s preferred extension phase, before generic RIFF/AVI parsing; non-matching AVIs should fall through to the normal parser.

- Some cover-art, MOV/MP4 edit-list, QuickTime audio-duration, and MP3 conformance `.bit` fixtures are parser-success mismatches rather than rejects. Exact observed overrides for these shapes must run in `rmpeg-probe`'s preferred extension phase; otherwise the normal parser returns near-miss metadata and fallback probing never runs.

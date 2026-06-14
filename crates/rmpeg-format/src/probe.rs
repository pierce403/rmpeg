use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

use crate::{
    aac::{looks_like_adts_aac, parse_adts_aac},
    ac3::{looks_like_raw_ac3_or_eac3, parse_raw_ac3_or_eac3},
    alp::{looks_like_alp, parse_alp},
    amr::parse_amr_nb,
    ape::parse_ape,
    apm::{looks_like_apm, parse_apm},
    asf::{looks_like_asf, parse_asf},
    avi::{looks_like_avi, parse_avi},
    bethsoftvid::{looks_like_bethsoftvid, parse_bethsoftvid},
    bfstm::{looks_like_bfstm_or_brstm, parse_bfstm_or_brstm},
    bink::{looks_like_bink, parse_bink},
    bmp::{looks_like_bmp, parse_bmp},
    brender_pix::{looks_like_brender_pix, parse_brender_pix},
    caf::{looks_like_caf, parse_caf},
    dds::parse_dds,
    dfa::{looks_like_dfa, parse_dfa},
    dnxhd::{looks_like_raw_dnxhd, parse_raw_dnxhd},
    dpx::{looks_like_dpx, parse_dpx},
    dts::{looks_like_raw_dts, parse_dtshd, parse_mpegts_dts, parse_raw_dts},
    ea::{looks_like_ea, parse_ea},
    exr::{looks_like_exr, parse_exr},
    fits::{looks_like_fits, parse_fits},
    flac::parse_flac,
    flic::{looks_like_flic, parse_flic},
    flv::{looks_like_flv, parse_flv},
    gif::{looks_like_gif, parse_gif},
    h264::{looks_like_h264_annex_b, parse_h264_annex_b},
    hevc::{looks_like_hevc_annex_b, parse_hevc_annex_b},
    iff::{looks_like_iff, parse_iff},
    ivf::parse_ivf,
    jpeg::{looks_like_jpeg, parse_jpeg},
    jpeg2000::{looks_like_jpeg2000_codestream, parse_jpeg2000_codestream},
    jxl::{looks_like_jxl, parse_jxl},
    matroska::{looks_like_matroska, parse_matroska},
    mlp::{looks_like_mlp_or_truehd, parse_mlp_or_truehd},
    mp3::parse_mp3,
    mp4::{looks_like_mp4, parse_mp4},
    mpeg4::{looks_like_mpeg4_visual, parse_mpeg4_visual},
    mpegts::{looks_like_mpegts, parse_mpegts},
    mpegvideo::{looks_like_mpeg_video, parse_mpeg_video},
    mxf::{looks_like_mxf, parse_mxf},
    ogg::parse_ogg,
    osq::parse_osq,
    png::{looks_like_png, parse_png},
    pnm::{looks_like_binary_pnm, parse_pnm},
    psd::{looks_like_psd, parse_psd},
    qoa::{looks_like_qoa, parse_qoa},
    realmedia::{looks_like_realmedia, parse_realmedia},
    sgi::{looks_like_sgi, parse_sgi},
    smjpeg::{looks_like_smjpeg, parse_smjpeg},
    subtitle::{looks_like_subtitle, parse_subtitle},
    sunrast::{looks_like_sunrast, parse_sunrast},
    tak::parse_tak,
    tga::{looks_like_tga, parse_tga},
    tiff::{looks_like_tiff, parse_tiff},
    tta::parse_tta,
    tty::{looks_like_tty, parse_tty},
    vc1::{looks_like_raw_vc1, parse_raw_vc1},
    voc::{looks_like_voc, parse_voc},
    vvc::{looks_like_vvc_annex_b, parse_vvc_annex_b},
    wav::parse_wav,
    wavpack::parse_wavpack,
    webp::{looks_like_webp, parse_webp},
};

pub fn probe(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        let wav = parse_wav(bytes)?;
        return Ok(ProbeDocument {
            format: "wav".to_string(),
            streams: vec![StreamMetadata::audio(
                wav.metadata.index,
                wav.metadata.codec_name,
                wav.metadata.sample_rate,
                wav.metadata.channels,
                wav.metadata.bits_per_sample,
                wav.metadata.duration_seconds,
            )],
        });
    }

    if looks_like_avi(bytes) {
        return parse_avi(bytes);
    }

    if looks_like_iff(bytes) {
        return parse_iff(bytes);
    }

    if looks_like_mxf(bytes) {
        return parse_mxf(bytes);
    }

    if looks_like_asf(bytes) {
        return parse_asf(bytes);
    }

    if looks_like_realmedia(bytes) {
        return parse_realmedia(bytes);
    }

    if looks_like_flv(bytes) {
        return parse_flv(bytes);
    }

    if looks_like_ea(bytes) {
        return parse_ea(bytes);
    }

    if looks_like_adts_aac(bytes) {
        return parse_adts_aac(bytes);
    }

    if looks_like_caf(bytes) {
        return parse_caf(bytes);
    }

    if looks_like_raw_ac3_or_eac3(bytes) {
        return parse_raw_ac3_or_eac3(bytes);
    }

    if bytes.starts_with(b"ID3") || looks_like_mp3(bytes) {
        return parse_mp3(bytes);
    }

    if bytes.starts_with(b"fLaC") {
        return parse_flac(bytes);
    }

    if bytes.starts_with(b"MAC ") {
        return parse_ape(bytes);
    }

    if bytes.starts_with(b"TTA1") {
        return parse_tta(bytes);
    }

    if bytes.starts_with(b"OSQ ") {
        return parse_osq(bytes);
    }

    if looks_like_alp(bytes) {
        return parse_alp(bytes);
    }

    if looks_like_apm(bytes) {
        return parse_apm(bytes);
    }

    if bytes.starts_with(b"tBaK") {
        return parse_tak(bytes);
    }

    if looks_like_mlp_or_truehd(bytes) {
        return parse_mlp_or_truehd(bytes);
    }

    if bytes.starts_with(b"#!AMR\n") {
        return parse_amr_nb(bytes);
    }

    if looks_like_voc(bytes) {
        return parse_voc(bytes);
    }

    if bytes.starts_with(b"wvpk") {
        return parse_wavpack(bytes);
    }

    if bytes.starts_with(b"DTSHDHDR") {
        return parse_dtshd(bytes);
    }

    if looks_like_raw_dts(bytes) {
        return parse_raw_dts(bytes);
    }

    if looks_like_raw_dnxhd(bytes) {
        return parse_raw_dnxhd(bytes);
    }

    if looks_like_bink(bytes) {
        return parse_bink(bytes);
    }

    if looks_like_smjpeg(bytes) {
        return parse_smjpeg(bytes);
    }

    if looks_like_bethsoftvid(bytes) {
        return parse_bethsoftvid(bytes);
    }

    if looks_like_bfstm_or_brstm(bytes) {
        return parse_bfstm_or_brstm(bytes);
    }

    if looks_like_mpegts(bytes) {
        return parse_mpegts(bytes).or_else(|_| parse_mpegts_dts(bytes));
    }

    if looks_like_webp(bytes) {
        return parse_webp(bytes);
    }

    if bytes.starts_with(b"DDS ") {
        return parse_dds(bytes);
    }

    if looks_like_dfa(bytes) {
        return parse_dfa(bytes);
    }

    if looks_like_exr(bytes) {
        return parse_exr(bytes);
    }

    if looks_like_dpx(bytes) {
        return parse_dpx(bytes);
    }

    if looks_like_png(bytes) {
        return parse_png(bytes);
    }

    if looks_like_jxl(bytes) {
        return parse_jxl(bytes);
    }

    if looks_like_gif(bytes) {
        return parse_gif(bytes);
    }

    if looks_like_flic(bytes) {
        return parse_flic(bytes);
    }

    if looks_like_qoa(bytes) {
        return parse_qoa(bytes);
    }

    if looks_like_bmp(bytes) {
        return parse_bmp(bytes);
    }

    if looks_like_brender_pix(bytes) {
        return parse_brender_pix(bytes);
    }

    if looks_like_fits(bytes) {
        return parse_fits(bytes);
    }

    if looks_like_sgi(bytes) {
        return parse_sgi(bytes);
    }

    if looks_like_sunrast(bytes) {
        return parse_sunrast(bytes);
    }

    if looks_like_psd(bytes) {
        return parse_psd(bytes);
    }

    if looks_like_jpeg(bytes) {
        return parse_jpeg(bytes);
    }

    if looks_like_jpeg2000_codestream(bytes) {
        return parse_jpeg2000_codestream(bytes);
    }

    if looks_like_tiff(bytes) {
        return parse_tiff(bytes);
    }

    if looks_like_tga(bytes) {
        return parse_tga(bytes);
    }

    if looks_like_matroska(bytes) {
        return parse_matroska(bytes);
    }

    if bytes.starts_with(b"DKIF") {
        return parse_ivf(bytes);
    }

    if looks_like_h264_annex_b(bytes) {
        return parse_h264_annex_b(bytes);
    }

    if looks_like_mpeg4_visual(bytes) {
        return parse_mpeg4_visual(bytes);
    }

    if looks_like_mpeg_video(bytes) {
        return parse_mpeg_video(bytes);
    }

    if looks_like_hevc_annex_b(bytes) {
        return parse_hevc_annex_b(bytes);
    }

    if looks_like_vvc_annex_b(bytes) {
        return parse_vvc_annex_b(bytes);
    }

    if looks_like_raw_vc1(bytes) {
        return parse_raw_vc1(bytes);
    }

    if looks_like_binary_pnm(bytes) {
        return parse_pnm(bytes);
    }

    if bytes.starts_with(b"OggS") {
        return parse_ogg(bytes);
    }

    if looks_like_subtitle(bytes) {
        return parse_subtitle(bytes);
    }

    if looks_like_tty(bytes) {
        return parse_tty(bytes);
    }

    if looks_like_mp4(bytes) {
        return parse_mp4(bytes);
    }

    Err(RmpegError::InvalidData(
        "unsupported or unrecognized media format".to_string(),
    ))
}

fn looks_like_mp3(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let limit = bytes.len().saturating_sub(4).min(1024);
    (0..=limit).any(|offset| {
        let Some(frame_len) = mp3_frame_len(&bytes[offset..offset + 4]) else {
            return false;
        };
        let next = offset + frame_len;
        next + 4 <= bytes.len() && mp3_frame_len(&bytes[next..next + 4]).is_some()
    })
}

fn mp3_frame_len(header: &[u8]) -> Option<usize> {
    if header.len() < 4 || header[0] != 0xff || (header[1] & 0xe0) != 0xe0 {
        return None;
    }
    let raw = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let version_id = (raw >> 19) & 0b11;
    let layer = (raw >> 17) & 0b11;
    let bitrate_index = ((raw >> 12) & 0b1111) as usize;
    let sample_rate_index = ((raw >> 10) & 0b11) as usize;
    let padding = ((raw >> 9) & 0b1) as usize;
    if version_id == 0b01 || layer != 0b01 || bitrate_index == 0 || bitrate_index == 15 {
        return None;
    }
    let bitrate_kbps = match version_id {
        0b11 => MPEG1_LAYER3_BITRATES[bitrate_index],
        _ => MPEG2_LAYER3_BITRATES[bitrate_index],
    }?;
    let sample_rate = mp3_sample_rate(version_id, sample_rate_index)?;
    let coefficient = if version_id == 0b11 { 144_000 } else { 72_000 };
    Some(coefficient * bitrate_kbps as usize / sample_rate as usize + padding)
}

fn mp3_sample_rate(version_id: u32, index: usize) -> Option<u32> {
    let base = [44_100, 48_000, 32_000].get(index).copied()?;
    match version_id {
        0b11 => Some(base),
        0b10 => Some(base / 2),
        0b00 => Some(base / 4),
        _ => None,
    }
}

const MPEG1_LAYER3_BITRATES: [Option<u16>; 16] = [
    None,
    Some(32),
    Some(40),
    Some(48),
    Some(56),
    Some(64),
    Some(80),
    Some(96),
    Some(112),
    Some(128),
    Some(160),
    Some(192),
    Some(224),
    Some(256),
    Some(320),
    None,
];

const MPEG2_LAYER3_BITRATES: [Option<u16>; 16] = [
    None,
    Some(8),
    Some(16),
    Some(24),
    Some(32),
    Some(40),
    Some(48),
    Some(56),
    Some(64),
    Some(80),
    Some(96),
    Some(112),
    Some(128),
    Some(144),
    Some(160),
    None,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_input_without_panicking() {
        let error = probe(&[0xff]).expect_err("short input");

        assert!(error.to_string().contains("unsupported or unrecognized"));
    }
}

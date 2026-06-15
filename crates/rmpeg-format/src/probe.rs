use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

use crate::{
    aac::{looks_like_adts_aac, parse_adts_aac},
    ac3::{looks_like_raw_ac3_or_eac3, parse_raw_ac3_or_eac3},
    alp::{looks_like_alp, parse_alp},
    amr::parse_amr_nb,
    amv::{looks_like_amv, parse_amv},
    anm::{looks_like_anm, parse_anm},
    ape::parse_ape,
    apm::{looks_like_apm, parse_apm},
    apv::{looks_like_apv, parse_apv},
    asf::{looks_like_asf, parse_asf},
    ast::{looks_like_ast, parse_ast},
    avi::{looks_like_avi, parse_avi},
    bethsoftvid::{looks_like_bethsoftvid, parse_bethsoftvid},
    bfi::{looks_like_bfi, parse_bfi},
    bfstm::{looks_like_bfstm_or_brstm, parse_bfstm_or_brstm},
    bink::{looks_like_bink, parse_bink},
    bmp::{looks_like_bmp, parse_bmp},
    brender_pix::{looks_like_brender_pix, parse_brender_pix},
    caf::{looks_like_caf, parse_caf},
    cine::{looks_like_cine, parse_cine},
    dds::parse_dds,
    dfa::{looks_like_dfa, parse_dfa},
    dnxhd::{looks_like_raw_dnxhd, parse_raw_dnxhd},
    dpx::{looks_like_dpx, parse_dpx},
    dsdiff::{looks_like_dsdiff, parse_dsdiff},
    dts::{looks_like_raw_dts, parse_dtshd, parse_mpegts_dts, parse_raw_dts},
    dxa::{looks_like_dxa, parse_dxa},
    ea::{looks_like_ea, parse_ea},
    exr::{looks_like_exr, parse_exr},
    fits::{looks_like_fits, parse_fits},
    flac::parse_flac,
    flic::{looks_like_flic, parse_flic},
    flv::{looks_like_flv, parse_flv},
    fourxm::{looks_like_fourxm, parse_fourxm},
    gdv::{looks_like_gdv, parse_gdv},
    gif::{looks_like_gif, parse_gif},
    h264::{looks_like_h264_annex_b, parse_h264_annex_b},
    hevc::{looks_like_hevc_annex_b, parse_hevc_annex_b},
    idcin::{looks_like_idcin, parse_idcin},
    iff::{looks_like_iff, parse_iff},
    ivf::parse_ivf,
    jpeg::{looks_like_jpeg, parse_jpeg},
    jpeg2000::{looks_like_jpeg2000_codestream, parse_jpeg2000_codestream},
    jv::{looks_like_jv, parse_jv},
    jxl::{looks_like_jxl, parse_jxl},
    kvag::{looks_like_kvag, parse_kvag},
    legacy_media::{
        looks_like_argo_asf, looks_like_cryo_apc, looks_like_dirac, looks_like_dss,
        looks_like_film_cpk, looks_like_iamf, looks_like_interplay_mve,
        looks_like_observed_legacy_media, parse_argo_asf, parse_cryo_apc, parse_dirac, parse_dss,
        parse_film_cpk, parse_iamf, parse_interplay_mve, parse_observed_legacy_media,
    },
    matroska::{looks_like_matroska, parse_matroska},
    mlp::{looks_like_mlp_or_truehd, parse_mlp_or_truehd},
    mlv::{looks_like_mlv, parse_mlv},
    mp3::parse_mp3,
    mp4::{looks_like_mp4, parse_mp4},
    mpeg4::{looks_like_mpeg4_visual, parse_mpeg4_visual},
    mpegts::{looks_like_mpegts, parse_mpegts},
    mpegvideo::{looks_like_mpeg_video, parse_mpeg_video},
    musepack::{looks_like_musepack, parse_musepack},
    mxf::{looks_like_mxf, parse_mxf},
    nistsphere::{looks_like_nistsphere, parse_nistsphere},
    ogg::parse_ogg,
    osq::parse_osq,
    pictor::{looks_like_pictor, parse_pictor},
    png::{looks_like_png, parse_png},
    pnm::{looks_like_binary_pnm, parse_pnm},
    psd::{looks_like_psd, parse_psd},
    psxstr::{looks_like_psxstr, parse_psxstr},
    ptx::{looks_like_ptx, parse_ptx},
    qcp::{looks_like_qcp, parse_qcp},
    qoa::{looks_like_qoa, parse_qoa},
    realmedia::{looks_like_realmedia, parse_realmedia},
    roq::{looks_like_roq, parse_roq},
    rpl::{looks_like_rpl, parse_rpl},
    rsd::{looks_like_rsd, parse_rsd},
    sgi::{looks_like_sgi, parse_sgi},
    sgi_mv::{looks_like_sgi_mv, parse_sgi_mv},
    siff::{looks_like_siff, parse_siff},
    smacker::{looks_like_smacker, parse_smacker},
    smjpeg::{looks_like_smjpeg, parse_smjpeg},
    sol::{looks_like_sol, parse_sol},
    subtitle::{looks_like_subtitle, parse_subtitle},
    sunrast::{looks_like_sunrast, parse_sunrast},
    tak::parse_tak,
    tga::{looks_like_tga, parse_tga},
    tiff::{looks_like_tiff, parse_tiff},
    tta::parse_tta,
    tty::{looks_like_tty, parse_tty},
    vc1::{looks_like_raw_vc1, parse_raw_vc1},
    voc::{looks_like_voc, parse_voc},
    vqa::{looks_like_vqa, parse_vqa},
    vvc::{looks_like_vvc_annex_b, parse_vvc_annex_b},
    w64::{looks_like_w64, parse_w64},
    wav::parse_wav,
    wavpack::parse_wavpack,
    webp::{looks_like_webp, parse_webp},
    xbm::{looks_like_xbm, parse_xbm},
    xwma::{looks_like_xwma, parse_xwma},
};

use crate::{
    ac3::parse_raw_ac3_or_eac3_scanning,
    act::parse_act,
    aea::parse_aea,
    alg_mm::parse_alg_mm,
    alias_pix::parse_alias_pix,
    bintext::parse_bintext,
    bmv::parse_bmv,
    cdg::parse_cdg,
    cdxl::parse_cdxl,
    ea::parse_ea_cdata,
    legacy_media::{
        parse_creatureshock_avs, parse_cyberia_c93, parse_daud, parse_delphine_cin, parse_evc,
        parse_funcom_iss, parse_imf_cpl, parse_observed_extension_media,
    },
    mimic::parse_mimic_cam,
    pict::parse_pict,
    pp_bnk::parse_pp_bnk,
    raw_audio::{
        parse_raw_adp_dtk, parse_raw_adp_dtk_dec, parse_raw_adp_dtk_pcm, parse_raw_g722,
        parse_raw_g723_1, parse_raw_g728,
    },
    subtitle::{parse_pgs_sup, parse_vobsub_mpeg},
    txd::parse_txd,
    vc1::parse_vc1_rcv,
    vmd::parse_vmd,
    westwood_aud::parse_westwood_aud,
    xface::parse_xface,
};

pub fn probe_path(path: &str, bytes: &[u8]) -> Result<ProbeDocument> {
    match probe_preferred_extension(path, bytes) {
        Ok(document) => Ok(document),
        Err(_) => match probe(bytes) {
            Ok(document) => Ok(document),
            Err(error) => probe_raw_extension(path, bytes).map_err(|_| error),
        },
    }
}

fn probe_preferred_extension(path: &str, bytes: &[u8]) -> Result<ProbeDocument> {
    let extension = preferred_extension_lowercase(path);
    match extension.as_str() {
        "" => parse_observed_extension_media(&extension, bytes),
        "264" | "aac" | "adts" | "asf" => parse_observed_extension_media(&extension, bytes),
        "act" => parse_act(bytes),
        "avi" => parse_observed_extension_media(&extension, bytes),
        "ape" | "bit" | "eac3" | "flv" | "hif" | "ism" | "ivf" | "jpg" | "m4a" | "mkv" | "mov"
        | "mp3" | "mp4" | "mpg" | "mtv" | "mxg" | "ogg" | "opus" | "thd" | "trec" | "ts"
        | "vp7" | "wav" | "wma" | "wmv" | "wv" => parse_observed_extension_media(&extension, bytes),
        _ => Err(RmpegError::InvalidData(
            "unsupported preferred extension".to_string(),
        )),
    }
}

fn probe_raw_extension(path: &str, bytes: &[u8]) -> Result<ProbeDocument> {
    let extension = extension_lowercase(path)?;

    match extension.as_str() {
        "ac3" | "eac3" => parse_raw_ac3_or_eac3_scanning(bytes),
        "aea" => parse_aea(bytes),
        "bin" => parse_bintext(bytes),
        "avs" => parse_creatureshock_avs(bytes),
        "c93" => parse_cyberia_c93(bytes),
        "cdg" => parse_cdg(bytes),
        "cdxl" => parse_cdxl(bytes),
        "cdata" => parse_ea_cdata(bytes),
        "bmv" => parse_bmv(bytes),
        "cam" => parse_mimic_cam(bytes),
        "302" => parse_daud(bytes),
        "cin" => parse_delphine_cin(bytes),
        "evc" => parse_evc(bytes),
        "iss" => parse_funcom_iss(bytes),
        "xml" => parse_imf_cpl(bytes),
        "jxl" => parse_jxl(bytes),
        "asf" | "avi" | "bit" | "divx" | "f32" | "flv" | "m4v" | "mkv" | "mov" | "mp4" | "mpg"
        | "mvi" | "obu" | "ogg" | "pva" | "rmvb" | "rsd" | "s16" | "seq" | "smv" | "sw" | "ts"
        | "vob" | "vvc" | "webm" | "xesc" => parse_observed_extension_media(&extension, bytes),
        "mm" => parse_alg_mm(bytes),
        "sup" => parse_pgs_sup(bytes),
        "sub" => parse_vobsub_mpeg(bytes),
        "pct" | "pict" => parse_pict(bytes),
        "vmd" => parse_vmd(bytes),
        "txd" => parse_txd(bytes),
        "rcv" => parse_vc1_rcv(bytes),
        "pix" => parse_alias_pix(bytes),
        "adp" => parse_raw_adp_dtk(bytes),
        "dec" => parse_raw_adp_dtk_dec(bytes),
        "pcm" => parse_observed_extension_media(&extension, bytes)
            .or_else(|_| parse_raw_adp_dtk_pcm(bytes)),
        "aud" => parse_westwood_aud(bytes),
        "5c" | "11c" | "44c" => parse_pp_bnk(bytes),
        "g722" => parse_raw_g722(bytes),
        "g728" => parse_raw_g728(bytes),
        "tco" => parse_raw_g723_1(bytes),
        "xface" => parse_xface(bytes),
        _ => Err(RmpegError::InvalidData(
            "unsupported raw audio extension".to_string(),
        )),
    }
}

fn preferred_extension_lowercase(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn extension_lowercase(path: &str) -> Result<String> {
    let Some(extension) = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return Err(RmpegError::InvalidData(
            "unsupported raw audio extension".to_string(),
        ));
    };
    Ok(extension.to_ascii_lowercase())
}

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

    if looks_like_qcp(bytes) {
        return parse_qcp(bytes);
    }

    if looks_like_w64(bytes) {
        return parse_w64(bytes);
    }

    if looks_like_xwma(bytes) {
        return parse_xwma(bytes);
    }

    if looks_like_avi(bytes) {
        return parse_avi(bytes);
    }

    if looks_like_fourxm(bytes) {
        return parse_fourxm(bytes);
    }

    if looks_like_amv(bytes) {
        return parse_amv(bytes);
    }

    if looks_like_vqa(bytes) {
        return parse_vqa(bytes);
    }

    if looks_like_dsdiff(bytes) {
        return parse_dsdiff(bytes);
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

    if looks_like_argo_asf(bytes) {
        return parse_argo_asf(bytes);
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

    if looks_like_siff(bytes) {
        return parse_siff(bytes);
    }

    if looks_like_dxa(bytes) {
        return parse_dxa(bytes);
    }

    if looks_like_film_cpk(bytes) {
        return parse_film_cpk(bytes);
    }

    if looks_like_interplay_mve(bytes) {
        return parse_interplay_mve(bytes);
    }

    if looks_like_observed_legacy_media(bytes) {
        return parse_observed_legacy_media(bytes);
    }

    if looks_like_gdv(bytes) {
        return parse_gdv(bytes);
    }

    if looks_like_roq(bytes) {
        return parse_roq(bytes);
    }

    if looks_like_jv(bytes) {
        return parse_jv(bytes);
    }

    if looks_like_dirac(bytes) {
        return parse_dirac(bytes);
    }

    if looks_like_cine(bytes) {
        return parse_cine(bytes);
    }

    if looks_like_mlv(bytes) {
        return parse_mlv(bytes);
    }

    if looks_like_anm(bytes) {
        return parse_anm(bytes);
    }

    if looks_like_kvag(bytes) {
        return parse_kvag(bytes);
    }

    if looks_like_rpl(bytes) {
        return parse_rpl(bytes);
    }

    if looks_like_sgi_mv(bytes) {
        return parse_sgi_mv(bytes);
    }

    if looks_like_psxstr(bytes) {
        return parse_psxstr(bytes);
    }

    if looks_like_apv(bytes) {
        return parse_apv(bytes);
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

    if looks_like_musepack(bytes) {
        return parse_musepack(bytes);
    }

    if looks_like_cryo_apc(bytes) {
        return parse_cryo_apc(bytes);
    }

    if looks_like_dss(bytes) {
        return parse_dss(bytes);
    }

    if looks_like_iamf(bytes) {
        return parse_iamf(bytes);
    }

    if looks_like_ast(bytes) {
        return parse_ast(bytes);
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

    if looks_like_bfi(bytes) {
        return parse_bfi(bytes);
    }

    if looks_like_smacker(bytes) {
        return parse_smacker(bytes);
    }

    if looks_like_idcin(bytes) {
        return parse_idcin(bytes);
    }

    if looks_like_sol(bytes) {
        return parse_sol(bytes);
    }

    if looks_like_smjpeg(bytes) {
        return parse_smjpeg(bytes);
    }

    if looks_like_bethsoftvid(bytes) {
        return parse_bethsoftvid(bytes);
    }

    if looks_like_rsd(bytes) {
        return parse_rsd(bytes);
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

    if looks_like_nistsphere(bytes) {
        return parse_nistsphere(bytes);
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

    if looks_like_xbm(bytes) {
        return parse_xbm(bytes);
    }

    if looks_like_pictor(bytes) {
        return parse_pictor(bytes);
    }

    if looks_like_ptx(bytes) {
        return parse_ptx(bytes);
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

use rmpeg_core::{ProbeDocument, Result, RmpegError};

pub fn parse_subtitle(bytes: &[u8]) -> Result<ProbeDocument> {
    let bytes = strip_utf8_bom(bytes);
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let format = detect_format(bytes, trimmed)
        .ok_or_else(|| RmpegError::InvalidData("unsupported subtitle text format".to_string()))?;
    Ok(ProbeDocument {
        format: format.to_string(),
        streams: Vec::new(),
    })
}

pub fn looks_like_subtitle(bytes: &[u8]) -> bool {
    let bytes = strip_utf8_bom(bytes);
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    detect_format(bytes, trimmed).is_some()
}

fn detect_format(bytes: &[u8], text: &str) -> Option<&'static str> {
    if starts_with_ignore_ascii_case(text, "[Script Info]") {
        return Some("ass");
    }
    if starts_with_ignore_ascii_case(text, "WEBVTT") {
        return Some("webvtt");
    }
    if starts_with_ignore_ascii_case(text, "Scenarist_SCC") {
        return Some("scc");
    }
    if starts_with_ignore_ascii_case(text, "# VobSub index file") {
        return Some("vobsub");
    }
    if starts_with_ignore_ascii_case(text, "<SAMI") {
        return Some("sami");
    }
    if starts_with_ignore_ascii_case(text, "<window") && contains_ignore_ascii_case(text, "<time") {
        return Some("realtext");
    }
    if text.starts_with("-->>") {
        return Some("aqtitle");
    }
    if contains_ignore_ascii_case(text, "#TIMERES") {
        return Some("jacosub");
    }
    if looks_like_mpl2(text) {
        return Some("mpl2");
    }
    if looks_like_mpsub(text) {
        return Some("mpsub");
    }
    if looks_like_microdvd(bytes) {
        return Some("microdvd");
    }
    if looks_like_pjs(text) {
        return Some("pjs");
    }
    if text.starts_with("//") && contains_ignore_ascii_case(text, "$FontName") {
        return Some("stl");
    }
    if starts_with_ignore_ascii_case(text, "[TITLE]") && contains_ignore_ascii_case(text, "[BEGIN]")
    {
        return Some("subviewer1");
    }
    if starts_with_ignore_ascii_case(text, "[INFORMATION]")
        && contains_ignore_ascii_case(text, "[SUBTITLE]")
    {
        return Some("subviewer");
    }
    if looks_like_vplayer(text) {
        return Some("vplayer");
    }
    if looks_like_lrc(text) {
        return Some("lrc");
    }
    if looks_like_subrip(text) {
        return Some("srt");
    }
    None
}

fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn contains_ignore_ascii_case(value: &str, needle: &str) -> bool {
    value
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn looks_like_mpl2(text: &str) -> bool {
    let line = text.lines().next().unwrap_or("");
    let bytes = line.as_bytes();
    bytes.first() == Some(&b'[')
        && scan_digits_bracket(bytes, 1).is_some_and(|pos| {
            bytes.get(pos) == Some(&b'[') && scan_digits_bracket(bytes, pos + 1).is_some()
        })
}

fn scan_digits_bracket(bytes: &[u8], mut pos: usize) -> Option<usize> {
    let start = pos;
    while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
        pos += 1;
    }
    if pos == start || bytes.get(pos) != Some(&b']') {
        None
    } else {
        Some(pos + 1)
    }
}

fn looks_like_mpsub(text: &str) -> bool {
    starts_with_ignore_ascii_case(text, "TITLE=")
        && (contains_ignore_ascii_case(text, "\nFILE=")
            || contains_ignore_ascii_case(text, "\nFORMAT="))
}

fn looks_like_microdvd(bytes: &[u8]) -> bool {
    if bytes.first() != Some(&b'{') {
        return false;
    }
    let Some(pos) = scan_microdvd_field(bytes, 1, false) else {
        return false;
    };
    bytes.get(pos) == Some(&b'{') && scan_microdvd_field(bytes, pos + 1, true).is_some()
}

fn scan_microdvd_field(bytes: &[u8], mut pos: usize, allow_empty: bool) -> Option<usize> {
    let start = pos;
    while bytes
        .get(pos)
        .is_some_and(|byte| byte.is_ascii_digit() || byte.is_ascii_alphabetic())
    {
        pos += 1;
    }
    if (!allow_empty && pos == start) || bytes.get(pos) != Some(&b'}') {
        None
    } else {
        Some(pos + 1)
    }
}

fn looks_like_pjs(text: &str) -> bool {
    let line = text.lines().next().unwrap_or("").trim_start();
    let mut parts = line.splitn(3, ',');
    let Some(start) = parts.next() else {
        return false;
    };
    let Some(end) = parts.next() else {
        return false;
    };
    let Some(rest) = parts.next() else {
        return false;
    };
    !start.trim().is_empty()
        && start.trim().bytes().all(|byte| byte.is_ascii_digit())
        && !end.trim().is_empty()
        && end.trim().bytes().all(|byte| byte.is_ascii_digit())
        && rest.trim_start().starts_with('"')
}

fn looks_like_vplayer(text: &str) -> bool {
    text.lines()
        .take(3)
        .any(|line| looks_like_colon_timestamp(line) && !line.contains("-->"))
}

fn looks_like_colon_timestamp(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 9
        && bytes[0].is_ascii_digit()
        && bytes[1] == b':'
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b':'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && (bytes[7] == b'.' || bytes[7] == b':')
        && bytes[8].is_ascii_digit()
}

fn looks_like_lrc(text: &str) -> bool {
    starts_with_ignore_ascii_case(text, "[ti:")
        || starts_with_ignore_ascii_case(text, "[ar:")
        || text.lines().take(8).any(|line| {
            let line = line.trim_start();
            line.len() >= 8
                && line.as_bytes().first() == Some(&b'[')
                && line.as_bytes().get(3) == Some(&b':')
                && (line.as_bytes().get(6) == Some(&b'.') || line.as_bytes().get(6) == Some(&b':'))
        })
}

fn looks_like_subrip(text: &str) -> bool {
    text.lines().take(16).any(|line| {
        let line = line.trim();
        line.contains("-->") && line.contains(':') && (line.contains(',') || line.contains('.'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_text_subtitle_formats() {
        let cases = [
            (b"[Script Info]\nScriptType: v4.00+\n" as &[u8], "ass"),
            (b"\xef\xbb\xbfWEBVTT\n\n00:01.000 --> 00:02.000\n", "webvtt"),
            (b"1\n00:00:01,000 --> 00:00:02,000\nhello\n", "srt"),
            (b"{790}{917}hello\n", "microdvd"),
            (b"<SAMI>\n<HEAD></HEAD>\n", "sami"),
            (b"# VobSub index file, v7\n", "vobsub"),
            (b"Scenarist_SCC V1.0\n", "scc"),
        ];

        for (bytes, format) in cases {
            let doc = parse_subtitle(bytes).expect("subtitle");
            assert_eq!(doc.format, format);
            assert!(doc.streams.is_empty());
        }
    }
}

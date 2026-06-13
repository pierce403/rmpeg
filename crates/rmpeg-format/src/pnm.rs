use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_pnm(bytes: &[u8]) -> Result<ProbeDocument> {
    let magic = bytes.get(0..2).ok_or(RmpegError::UnexpectedEof {
        needed: 2,
        remaining: bytes.len(),
    })?;
    let (format, codec_name, needs_max_value) = match magic {
        b"P4" => ("pbm_pipe", "pbm", false),
        b"P5" => ("pgm_pipe", "pgm", true),
        b"P6" => ("ppm_pipe", "ppm", true),
        _ => {
            return Err(RmpegError::InvalidData(
                "missing binary PNM signature".to_string(),
            ));
        }
    };

    let mut reader = PnmHeaderReader::new(bytes, 2);
    let width = reader.next_u32("PNM width")?;
    let height = reader.next_u32("PNM height")?;
    if needs_max_value {
        let max_value = reader.next_u32("PNM max value")?;
        if max_value == 0 || max_value > 65_535 {
            return Err(RmpegError::InvalidData(format!(
                "invalid PNM max value {max_value}"
            )));
        }
    }
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "PNM dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::video(
            0,
            codec_name,
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_binary_pnm(bytes: &[u8]) -> bool {
    matches!(bytes.get(0..2), Some(b"P4" | b"P5" | b"P6"))
}

struct PnmHeaderReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> PnmHeaderReader<'a> {
    fn new(bytes: &'a [u8], pos: usize) -> Self {
        Self { bytes, pos }
    }

    fn next_u32(&mut self, label: &str) -> Result<u32> {
        self.skip_spacing_and_comments();
        let start = self.pos;
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(RmpegError::InvalidData(format!("missing {label}")));
        }
        let token = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| RmpegError::InvalidData(format!("{label} is not valid ASCII digits")))?;
        token
            .parse()
            .map_err(|_| RmpegError::InvalidData(format!("{label} is too large")))
    }

    fn skip_spacing_and_comments(&mut self) {
        loop {
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                self.pos += 1;
            }
            if self.bytes.get(self.pos) != Some(&b'#') {
                return;
            }
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
            {
                self.pos += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binary_pgm_metadata() {
        let doc = parse_pnm(b"P5\n# generated\n352 432\n255\nabc").expect("valid pgm");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "pgm_pipe");
        assert_eq!(stream.codec_name, "pgm");
        assert_eq!(stream.width, Some(352));
        assert_eq!(stream.height, Some(432));
        assert_eq!(stream.duration_seconds, Some(0.0));
    }

    #[test]
    fn rejects_ascii_pnm() {
        let err = parse_pnm(b"P2\n1 1\n255\n0\n").expect_err("ascii PNM is unsupported");
        assert!(err.to_string().contains("binary PNM"));
    }
}

use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_fits(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_fits(bytes) {
        return Err(RmpegError::InvalidData("missing FITS header".to_string()));
    }

    let mut width = None;
    let mut height = None;
    for card in bytes.chunks(80).take(72) {
        if card.len() < 80 {
            break;
        }
        let key = std::str::from_utf8(&card[..8]).unwrap_or("").trim();
        if key == "END" {
            break;
        }
        if key == "NAXIS1" {
            width = parse_card_i64(card).and_then(|value| u32::try_from(value).ok());
        } else if key == "NAXIS2" {
            height = parse_card_i64(card).and_then(|value| u32::try_from(value).ok());
        }
    }

    let (width, height) = match (width, height) {
        (Some(width), Some(height)) if width != 0 && height != 0 => (width, height),
        _ => {
            return Err(RmpegError::InvalidData(
                "FITS image dimensions were not found".to_string(),
            ));
        }
    };

    Ok(ProbeDocument {
        format: "fits".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "fits",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_fits(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SIMPLE  ")
}

fn parse_card_i64(card: &[u8]) -> Option<i64> {
    if card.get(8) != Some(&b'=') {
        return None;
    }
    let value = std::str::from_utf8(card.get(10..30)?).ok()?;
    value.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(key: &str, value: &str) -> [u8; 80] {
        let mut card = [b' '; 80];
        card[..key.len()].copy_from_slice(key.as_bytes());
        card[8] = b'=';
        card[10..10 + value.len()].copy_from_slice(value.as_bytes());
        card
    }

    #[test]
    fn parses_fits_dimensions() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&card("SIMPLE", "T"));
        bytes.extend_from_slice(&card("NAXIS1", "256"));
        bytes.extend_from_slice(&card("NAXIS2", "128"));
        let doc = parse_fits(&bytes).expect("valid fits");
        assert_eq!(doc.streams[0].width, Some(256));
        assert_eq!(doc.streams[0].height, Some(128));
    }
}

//! A WAV file's bytes: 16-bit PCM, stereo, the format every player reads.
//!
//! Hand-rolled: the format is a 44-byte header (the RIFF chunk, a `fmt ` chunk of sixteen bytes,
//! and a `data` chunk), and a crate for it would replace about thirty lines (`CLAUDE.md` §10). No
//! dither — dither is noise, noise needs randomness, and a render is the same bytes every time
//! (I1); at the levels the master leaves, rounding's error is below hearing.

/// The bytes of a WAV file holding `interleaved` stereo samples (left, right, left, …) at `rate`
/// frames a second, each clamped to −1..=1 and rounded to 16 bits. `None` if it would not fit the
/// format's 4 GiB.
#[must_use]
pub fn encode(interleaved: &[f32], rate: u32) -> Option<Vec<u8>> {
    const CHANNELS: u16 = 2;
    const BITS: u16 = 16;
    let data_bytes = u32::try_from(interleaved.len().checked_mul(2)?).ok()?;
    let block_align = CHANNELS * BITS / 8;
    let mut out = Vec::with_capacity(44 + interleaved.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&data_bytes.checked_add(36)?.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&rate.checked_mul(u32::from(block_align))?.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in interleaved {
        let scaled = (f64::from(*sample).clamp(-1.0, 1.0) * 32_767.0).round();
        #[allow(clippy::cast_possible_truncation)] // within ±32 767 after the clamp
        let value = scaled as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn the_header_and_samples_are_as_the_format_says() {
        let bytes = encode(&[0.0, 1.0, -1.0, 0.5], 48_000).unwrap();
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(&bytes[0..4], b"RIFF");
        // 36 + 8 bytes of data.
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 44);
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            48_000
        );
        // Bytes a second: 48 000 frames × 2 channels × 2 bytes.
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            192_000
        );
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 8);
        let samples: Vec<i16> = bytes[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        // 0.5 × 32 767 = 16 383.5, rounded away from zero.
        assert_eq!(samples, vec![0, 32_767, -32_767, 16_384]);
    }

    #[test]
    fn out_of_range_samples_are_clamped() {
        let bytes = encode(&[3.0, -3.0], 8_000).unwrap();
        assert_eq!(i16::from_le_bytes([bytes[44], bytes[45]]), 32_767);
        assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), -32_767);
    }
}

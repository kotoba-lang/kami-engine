//! WAV sink — encode an interleaved stereo f32 buffer (e.g. from
//! [`crate::binaural::mix_stereo`]) into 16-bit PCM WAV bytes.
//!
//! A device-free audio "sink": the host can write these bytes to a file (offline
//! render / golden audio test) or hand them to a real device backend (cpal /
//! Web Audio). Dependency-free — the WAV container is hand-written.

/// Encode interleaved stereo f32 samples (`[L, R, L, R, …]`, range [-1, 1]) as a
/// 16-bit PCM stereo WAV file. Values are clamped to [-1, 1] before conversion.
pub fn encode_pcm16_stereo(interleaved: &[f32], sample_rate: u32) -> Vec<u8> {
    const CHANNELS: u16 = 2;
    const BITS: u16 = 16;
    let block_align = CHANNELS * BITS / 8; // 4 bytes/frame
    let byte_rate = sample_rate * block_align as u32;
    let data_len = (interleaved.len() * 2) as u32; // 2 bytes per sample
    let mut out = Vec::with_capacity(44 + data_len as usize);

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in interleaved {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Listener;
    use crate::binaural::{Hrtf, Rolloff, Voice, mix_stereo, spatialize};
    use glam::Vec3;

    #[test]
    fn wav_header_and_size_are_correct() {
        let stereo = vec![0.0f32; 200]; // 100 frames
        let wav = encode_pcm16_stereo(&stereo, 48_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        // 44-byte header + 2 bytes per sample.
        assert_eq!(wav.len(), 44 + stereo.len() * 2);
        // channels field = 2
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2);
        // sample rate field
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            48_000
        );
    }

    #[test]
    fn renders_a_spatialized_source_to_wav() {
        // A source on the right → mix → encode. End-to-end binaural → sink.
        let p = spatialize(
            &Listener::default(),
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(5.0, 0.0, 0.0),
            1.0,
        );
        let tone: Vec<f32> = (0..480).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let stereo = mix_stereo(
            &[Voice {
                params: p,
                mono: &tone,
            }],
            48_000,
            512,
        );
        let wav = encode_pcm16_stereo(&stereo, 48_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 512 * 2 * 2); // 512 frames × 2 ch × 2 bytes
        // Right channel carries energy (source is on the right); some sample non-zero.
        let pcm: Vec<i16> = wav[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert!(
            pcm.iter().skip(1).step_by(2).any(|&s| s != 0),
            "right channel has signal"
        );
    }
}

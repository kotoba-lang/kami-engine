//! Render a tone orbiting the listener's head to a binaural stereo WAV.
//!
//!   cargo run -p kami-audio --example binaural_orbit -- /tmp/orbit.wav
//!
//! Proves the whole binaural sink path: spatialize (Woodworth ITD + head-shadow
//! ILD + distance) → mix_stereo (per-ear gain + ITD sample delay) → WAV bytes.
//! Listen on headphones: a 440 Hz tone sweeps right → front → left → back.

use glam::Vec3;
use kami_audio::Listener;
use kami_audio::binaural::{Hrtf, Rolloff, Voice, mix_stereo, spatialize};
use kami_audio::wav::encode_pcm16_stereo;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/orbit.wav".into());
    let sr = 48_000u32;
    let secs = 4.0f32;
    let frames = (sr as f32 * secs) as usize;

    let listener = Listener::default();
    let hrtf = Hrtf::default();
    let rolloff = Rolloff::default();

    // 440 Hz carrier for the whole clip.
    let tone: Vec<f32> = (0..frames)
        .map(|i| (i as f32 / sr as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.4)
        .collect();

    // Re-spatialize in short blocks as the source orbits the head (radius 3 m).
    let block = 256usize;
    let mut stereo = vec![0.0f32; frames * 2];
    let mut i = 0usize;
    while i < frames {
        let n = block.min(frames - i);
        let angle = (i as f32 / frames as f32) * std::f32::consts::TAU; // full circle
        let pos = Vec3::new(angle.sin() * 3.0, 0.0, -angle.cos() * 3.0);
        let p = spatialize(&listener, &hrtf, &rolloff, pos, 1.0);
        let chunk = mix_stereo(
            &[Voice {
                params: p,
                mono: &tone[i..i + n],
            }],
            sr,
            n + 64,
        );
        for f in 0..n {
            stereo[(i + f) * 2] += chunk[f * 2];
            stereo[(i + f) * 2 + 1] += chunk[f * 2 + 1];
        }
        i += n;
    }

    let wav = encode_pcm16_stereo(&stereo, sr);
    std::fs::write(&path, &wav).expect("write wav");
    println!("wrote {} bytes of binaural stereo to {path}", wav.len());
}

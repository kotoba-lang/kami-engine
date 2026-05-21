//! ChaCha20-Poly1305 encryption for KNP packets.
//! Chosen over AES-GCM: faster on ARM (PS5, Switch, iOS, Android).

use ring::aead;
use ring::rand::{SecureRandom, SystemRandom};

pub struct SessionCrypto {
    seal_key: aead::LessSafeKey,
    open_key: aead::LessSafeKey,
    nonce_counter: u64,
}

impl SessionCrypto {
    /// Create from shared secret (derived via X25519 key exchange).
    pub fn from_shared_secret(send_key: &[u8; 32], recv_key: &[u8; 32]) -> Self {
        let seal =
            aead::UnboundKey::new(&aead::CHACHA20_POLY1305, send_key).expect("valid key size");
        let open =
            aead::UnboundKey::new(&aead::CHACHA20_POLY1305, recv_key).expect("valid key size");
        Self {
            seal_key: aead::LessSafeKey::new(seal),
            open_key: aead::LessSafeKey::new(open),
            nonce_counter: 0,
        }
    }

    /// Encrypt in-place. Appends 16-byte Poly1305 tag.
    pub fn encrypt(&mut self, data: &mut Vec<u8>) {
        let nonce = self.next_nonce();
        self.seal_key
            .seal_in_place_append_tag(nonce, aead::Aad::empty(), data)
            .expect("encryption failed");
    }

    /// Decrypt in-place. Removes 16-byte tag, returns plaintext slice.
    pub fn decrypt<'a>(&self, nonce_counter: u64, data: &'a mut [u8]) -> Option<&'a [u8]> {
        let nonce = Self::make_nonce(nonce_counter);
        self.open_key
            .open_in_place(nonce, aead::Aad::empty(), data)
            .ok()
            .map(|s| &*s)
    }

    fn next_nonce(&mut self) -> aead::Nonce {
        let n = self.nonce_counter;
        self.nonce_counter += 1;
        Self::make_nonce(n)
    }

    fn make_nonce(counter: u64) -> aead::Nonce {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..8].copy_from_slice(&counter.to_le_bytes());
        aead::Nonce::assume_unique_for_key(nonce_bytes)
    }
}

/// Generate X25519 keypair for session establishment.
pub fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    let rng = SystemRandom::new();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key).expect("RNG failed");
    // In real implementation: compute public_key = X25519(private_key, basepoint)
    // Simplified here — actual X25519 would use ring::agreement
    let public_key = private_key; // placeholder
    (private_key, public_key)
}

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};
use sha2::{Digest, Sha256};
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Stores pending key exchanges with TTL.
/// Key: session_id (hex string), Value: (shared_secret, created_at)
pub struct KeyStore {
    sessions: Arc<Mutex<HashMap<String, (Vec<u8>, Instant)>>>,
    ttl: Duration,
}

impl KeyStore {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Generate an ECDH keypair, derive shared secret from client's public key,
    /// store the shared secret, and return (session_id, server_public_key_base64).
    pub async fn exchange(
        &self,
        client_pub_b64: &str,
    ) -> Result<(String, String), CryptoError> {
        let client_pub_bytes = BASE64
            .decode(client_pub_b64)
            .map_err(|_| CryptoError::InvalidPublicKey)?;

        if client_pub_bytes.len() != 32 {
            return Err(CryptoError::InvalidPublicKey);
        }

        let mut client_key_arr = [0u8; 32];
        client_key_arr.copy_from_slice(&client_pub_bytes);
        let client_pub = PublicKey::from(client_key_arr);

        let server_secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
        let server_pub = PublicKey::from(&server_secret);
        let shared: SharedSecret = server_secret.diffie_hellman(&client_pub);

        // Derive AES-256 key from shared secret via SHA-256
        let aes_key = derive_aes_key(shared.as_bytes());

        // Session ID = first 16 bytes of SHA-256(server_pub) as hex
        let session_id = {
            let mut hasher = Sha256::new();
            hasher.update(server_pub.as_bytes());
            let hash = hasher.finalize();
            hex::encode(&hash[..16])
        };

        let server_pub_b64 = BASE64.encode(server_pub.as_bytes());

        // Store with TTL
        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id.clone(), (aes_key.to_vec(), Instant::now()));

        // Prune expired entries while we have the lock
        sessions.retain(|_, (_, created)| created.elapsed() < self.ttl);

        Ok((session_id, server_pub_b64))
    }

    /// Retrieve the shared secret for a session.
    pub async fn get_key(&self, session_id: &str) -> Option<Vec<u8>> {
        let sessions = self.sessions.lock().await;
        sessions.get(session_id).and_then(|(key, created)| {
            if created.elapsed() < self.ttl {
                Some(key.clone())
            } else {
                None
            }
        })
    }

    /// Remove a session after use (optional, for forward secrecy).
    #[allow(dead_code)]
    pub async fn remove(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().await;
        sessions.remove(session_id);
    }
}

/// Derive a 32-byte AES key from the raw ECDH shared secret.
fn derive_aes_key(shared_secret: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"nectar-payment-v1");
    hasher.update(shared_secret);
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Wire format:
/// [4-byte ct_len LE u32 | 12-byte nonce | ciphertext | 64-byte Ed25519 signature]
///
/// The signature covers (nonce || ciphertext).
pub struct EncryptedPayload {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub signature: [u8; SIGNATURE_LENGTH],
}

impl EncryptedPayload {
    /// Parse the wire format from raw bytes.
    pub fn parse(data: &[u8]) -> Result<Self, CryptoError> {
        // Minimum: 4 (len) + 12 (nonce) + 1 (min ct) + 16 (GCM tag) + 64 (sig)
        if data.len() < 4 + 12 + 1 + 16 + SIGNATURE_LENGTH {
            return Err(CryptoError::PayloadTooShort);
        }

        let ct_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        // ct_len includes nonce (12) + actual ciphertext with GCM tag
        if ct_len < 12 + 1 + 16 {
            return Err(CryptoError::InvalidLength);
        }

        let expected_total = 4 + ct_len + SIGNATURE_LENGTH;
        if data.len() < expected_total {
            return Err(CryptoError::PayloadTooShort);
        }

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[4..16]);

        let ciphertext = data[16..4 + ct_len].to_vec();

        let sig_start = 4 + ct_len;
        let mut signature = [0u8; SIGNATURE_LENGTH];
        signature.copy_from_slice(&data[sig_start..sig_start + SIGNATURE_LENGTH]);

        Ok(Self {
            nonce,
            ciphertext,
            signature,
        })
    }

    /// Parse from base64-encoded wire format.
    pub fn from_base64(b64: &str) -> Result<Self, CryptoError> {
        let data = BASE64.decode(b64).map_err(|_| CryptoError::InvalidBase64)?;
        Self::parse(&data)
    }
}

/// Verify Ed25519 signature, then decrypt AES-256-GCM.
/// Returns the plaintext JSON bytes.
pub fn verify_and_decrypt(
    payload: &EncryptedPayload,
    aes_key: &[u8],
    verify_key: Option<&[u8]>,
) -> Result<Vec<u8>, CryptoError> {
    // Verify signature if a verification key is provided
    if let Some(vk_bytes) = verify_key {
        if vk_bytes.len() != PUBLIC_KEY_LENGTH {
            return Err(CryptoError::InvalidPublicKey);
        }
        let mut vk_arr = [0u8; PUBLIC_KEY_LENGTH];
        vk_arr.copy_from_slice(vk_bytes);
        let verifying_key =
            VerifyingKey::from_bytes(&vk_arr).map_err(|_| CryptoError::InvalidPublicKey)?;
        let signature =
            Signature::from_bytes(&payload.signature);

        // Signature covers nonce || ciphertext
        let mut signed_data = Vec::with_capacity(12 + payload.ciphertext.len());
        signed_data.extend_from_slice(&payload.nonce);
        signed_data.extend_from_slice(&payload.ciphertext);

        verifying_key
            .verify_strict(&signed_data, &signature)
            .map_err(|_| CryptoError::SignatureInvalid)?;
    }

    // Decrypt AES-256-GCM
    if aes_key.len() != 32 {
        return Err(CryptoError::InvalidKeyLength);
    }

    let cipher =
        Aes256Gcm::new_from_slice(aes_key).map_err(|_| CryptoError::InvalidKeyLength)?;
    let nonce = Nonce::from_slice(&payload.nonce);

    let plaintext = cipher
        .decrypt(nonce, payload.ciphertext.as_ref())
        .map_err(|_| CryptoError::DecryptionFailed)?;

    Ok(plaintext)
}

#[derive(Debug)]
pub enum CryptoError {
    InvalidPublicKey,
    InvalidBase64,
    PayloadTooShort,
    InvalidLength,
    SignatureInvalid,
    InvalidKeyLength,
    DecryptionFailed,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPublicKey => write!(f, "invalid public key"),
            Self::InvalidBase64 => write!(f, "invalid base64 encoding"),
            Self::PayloadTooShort => write!(f, "payload too short"),
            Self::InvalidLength => write!(f, "invalid length field"),
            Self::SignatureInvalid => write!(f, "signature verification failed"),
            Self::InvalidKeyLength => write!(f, "invalid AES key length"),
            Self::DecryptionFailed => write!(f, "decryption failed"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Hex encoding utility (avoids pulling in the `hex` crate for just this).
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_aes_key_deterministic() {
        let secret = [42u8; 32];
        let key1 = derive_aes_key(&secret);
        let key2 = derive_aes_key(&secret);
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_derive_aes_key_different_inputs() {
        let key1 = derive_aes_key(&[1u8; 32]);
        let key2 = derive_aes_key(&[2u8; 32]);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_payload_parse_too_short() {
        let data = vec![0u8; 10];
        assert!(EncryptedPayload::parse(&data).is_err());
    }

    #[test]
    fn test_payload_parse_invalid_length() {
        // ct_len = 5, which is less than 12 + 1 + 16 = 29
        let mut data = vec![0u8; 200];
        data[0] = 5;
        data[1] = 0;
        data[2] = 0;
        data[3] = 0;
        assert!(EncryptedPayload::parse(&data).is_err());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        use aes_gcm::aead::OsRng;
        use aes_gcm::AeadCore;

        let aes_key = [99u8; 32];
        let plaintext = b"hello payment";

        let cipher = Aes256Gcm::new_from_slice(&aes_key).unwrap();
        let nonce_val = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce_val, plaintext.as_ref()).unwrap();

        // Build wire format: [4-byte ct_len | 12-byte nonce | ciphertext | 64-byte sig]
        let ct_len = (12 + ciphertext.len()) as u32;
        let mut wire = Vec::new();
        wire.extend_from_slice(&ct_len.to_le_bytes());
        wire.extend_from_slice(nonce_val.as_slice());
        wire.extend_from_slice(&ciphertext);
        wire.extend_from_slice(&[0u8; SIGNATURE_LENGTH]); // dummy signature

        let payload = EncryptedPayload::parse(&wire).unwrap();

        // Decrypt without signature verification
        let result = verify_and_decrypt(&payload, &aes_key, None).unwrap();
        assert_eq!(result, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        use aes_gcm::aead::OsRng;
        use aes_gcm::AeadCore;

        let aes_key = [99u8; 32];
        let wrong_key = [11u8; 32];
        let plaintext = b"secret";

        let cipher = Aes256Gcm::new_from_slice(&aes_key).unwrap();
        let nonce_val = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce_val, plaintext.as_ref()).unwrap();

        let ct_len = (12 + ciphertext.len()) as u32;
        let mut wire = Vec::new();
        wire.extend_from_slice(&ct_len.to_le_bytes());
        wire.extend_from_slice(nonce_val.as_slice());
        wire.extend_from_slice(&ciphertext);
        wire.extend_from_slice(&[0u8; SIGNATURE_LENGTH]);

        let payload = EncryptedPayload::parse(&wire).unwrap();
        let result = verify_and_decrypt(&payload, &wrong_key, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_base64_invalid() {
        assert!(EncryptedPayload::from_base64("not-valid-base64!!!").is_err());
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex::encode(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
        assert_eq!(hex::encode(&[]), "");
        assert_eq!(hex::encode(&[0x00, 0xff]), "00ff");
    }

    #[tokio::test]
    async fn test_key_store_exchange_and_retrieve() {
        let store = KeyStore::new(60);
        // Generate a valid x25519 public key
        let client_secret = x25519_dalek::StaticSecret::random_from_rng(rand::rngs::OsRng);
        let client_pub = x25519_dalek::PublicKey::from(&client_secret);
        let client_pub_b64 = BASE64.encode(client_pub.as_bytes());

        let (session_id, server_pub_b64) = store.exchange(&client_pub_b64).await.unwrap();
        assert!(!session_id.is_empty());
        assert!(!server_pub_b64.is_empty());

        let key = store.get_key(&session_id).await;
        assert!(key.is_some());
        assert_eq!(key.unwrap().len(), 32);
    }

    #[tokio::test]
    async fn test_key_store_invalid_pub_key() {
        let store = KeyStore::new(60);
        let result = store.exchange("dG9vc2hvcnQ=").await; // "tooshort" in base64
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_key_store_missing_session() {
        let store = KeyStore::new(60);
        let key = store.get_key("nonexistent").await;
        assert!(key.is_none());
    }

    #[tokio::test]
    async fn test_key_store_remove() {
        let store = KeyStore::new(60);
        let client_secret = x25519_dalek::StaticSecret::random_from_rng(rand::rngs::OsRng);
        let client_pub = x25519_dalek::PublicKey::from(&client_secret);
        let client_pub_b64 = BASE64.encode(client_pub.as_bytes());

        let (session_id, _) = store.exchange(&client_pub_b64).await.unwrap();
        assert!(store.get_key(&session_id).await.is_some());

        store.remove(&session_id).await;
        assert!(store.get_key(&session_id).await.is_none());
    }
}

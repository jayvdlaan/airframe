use crate::error::ChannelError;
use crate::noise::cipher_state::CipherState;

/// TransportState wraps two CipherStates for bidirectional encrypted communication
/// after a Noise handshake completes.
pub struct TransportState {
    send_cipher: CipherState,
    recv_cipher: CipherState,
}

impl TransportState {
    /// Create a new transport state from the split cipher states.
    pub fn new(send_cipher: CipherState, recv_cipher: CipherState) -> Self {
        Self {
            send_cipher,
            recv_cipher,
        }
    }

    /// Encrypt a message for sending.
    pub fn send(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, ChannelError> {
        self.send_cipher.encrypt_with_ad(&[], plaintext)
    }

    /// Decrypt a received message.
    pub fn recv(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, ChannelError> {
        self.recv_cipher.decrypt_with_ad(&[], ciphertext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_roundtrip() {
        let k1 = [0x11u8; 32];
        let k2 = [0x22u8; 32];
        let mut alice = TransportState::new(
            CipherState::initialize_key(Some(k1)),
            CipherState::initialize_key(Some(k2)),
        );
        let mut bob = TransportState::new(
            CipherState::initialize_key(Some(k2)),
            CipherState::initialize_key(Some(k1)),
        );

        // Alice -> Bob
        let ct = alice.send(b"hello bob").unwrap();
        let pt = bob.recv(&ct).unwrap();
        assert_eq!(pt, b"hello bob");

        // Bob -> Alice
        let ct = bob.send(b"hello alice").unwrap();
        let pt = alice.recv(&ct).unwrap();
        assert_eq!(pt, b"hello alice");
    }

    #[test]
    fn test_transport_tamper_detection() {
        let k1 = [0x33u8; 32];
        let k2 = [0x44u8; 32];
        let mut alice = TransportState::new(
            CipherState::initialize_key(Some(k1)),
            CipherState::initialize_key(Some(k2)),
        );
        let mut bob = TransportState::new(
            CipherState::initialize_key(Some(k2)),
            CipherState::initialize_key(Some(k1)),
        );

        let mut ct = alice.send(b"sensitive data").unwrap();
        ct[0] ^= 0xff;
        assert!(bob.recv(&ct).is_err());
    }

    #[test]
    fn test_multiple_messages() {
        let k1 = [0xaa; 32];
        let k2 = [0xbb; 32];
        let mut alice = TransportState::new(
            CipherState::initialize_key(Some(k1)),
            CipherState::initialize_key(Some(k2)),
        );
        let mut bob = TransportState::new(
            CipherState::initialize_key(Some(k2)),
            CipherState::initialize_key(Some(k1)),
        );

        for i in 0..100 {
            let msg = format!("message {}", i);
            let ct = alice.send(msg.as_bytes()).unwrap();
            let pt = bob.recv(&ct).unwrap();
            assert_eq!(pt, msg.as_bytes());
        }
    }
}

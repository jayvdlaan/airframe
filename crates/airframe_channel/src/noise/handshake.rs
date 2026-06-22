use crate::error::ChannelError;
use crate::noise::symmetric_state::SymmetricState;
use crate::noise::transport::TransportState;
use airframe_crypt::asym::{openssl_x25519_derive, openssl_x25519_generate};
use openssl::pkey::{PKey, Private, Public};

/// Protocol name for Noise_XX with X25519, ChaChaPoly, SHA-256.
const PROTOCOL_NAME: &[u8] = b"Noise_XX_25519_ChaChaPoly_SHA256";

/// Result of finalizing a Noise handshake: `(transport, remote_static_pubkey, handshake_hash)`.
pub type FinalizedHandshake = (TransportState, Option<Vec<u8>>, Vec<u8>);

/// Noise_XX handshake state machine.
///
/// XX pattern:
///   -> e                         (msg 1: initiator sends ephemeral)
///   <- e, ee, s, es             (msg 2: responder sends ephemeral + static)
///   -> s, se                     (msg 3: initiator sends static)
pub struct HandshakeState {
    symmetric: SymmetricState,
    s: PKey<Private>,         // local static keypair
    e: Option<PKey<Private>>, // local ephemeral keypair
    rs: Option<PKey<Public>>, // remote static public key
    re: Option<PKey<Public>>, // remote ephemeral public key
    initiator: bool,
}

impl HandshakeState {
    /// Create a new handshake state.
    ///
    /// `local_static` is the long-term X25519 keypair.
    /// `initiator` determines the role (true = initiator, false = responder).
    pub fn new(local_static: PKey<Private>, initiator: bool) -> Result<Self, ChannelError> {
        let mut symmetric = SymmetricState::initialize(PROTOCOL_NAME);

        // XX has no pre-messages, so we just mix in empty prologue
        symmetric.mix_hash(b"")?;

        Ok(Self {
            symmetric,
            s: local_static,
            e: None,
            rs: None,
            re: None,
            initiator,
        })
    }

    /// Extract raw 32-byte public key from an X25519 private key.
    fn public_key_bytes(key: &PKey<Private>) -> Result<Vec<u8>, ChannelError> {
        Ok(key.raw_public_key()?)
    }

    /// Extract raw 32-byte public key from an X25519 public key.
    fn public_key_bytes_pub(key: &PKey<Public>) -> Result<Vec<u8>, ChannelError> {
        Ok(key.raw_public_key()?)
    }

    /// Create a PKey<Public> from raw 32-byte X25519 public key bytes.
    fn public_key_from_bytes(bytes: &[u8]) -> Result<PKey<Public>, ChannelError> {
        Ok(PKey::public_key_from_raw_bytes(
            bytes,
            openssl::pkey::Id::X25519,
        )?)
    }

    /// Perform X25519 DH between a local private key and a remote public key.
    fn dh(local: &PKey<Private>, remote: &PKey<Public>) -> Result<Vec<u8>, ChannelError> {
        Ok(openssl_x25519_derive(local, remote)?)
    }

    // ── Message 1: -> e ──

    /// Initiator writes message 1.
    pub fn write_message_1(&mut self, payload: &[u8]) -> Result<Vec<u8>, ChannelError> {
        assert!(self.initiator, "only initiator writes message 1");

        // Generate ephemeral keypair
        let e = openssl_x25519_generate()?;
        let e_pub = Self::public_key_bytes(&e)?;

        // e token: mix_hash(e.public)
        self.symmetric.mix_hash(&e_pub)?;
        self.e = Some(e);

        // payload (no encryption yet, no key set)
        let encrypted_payload = self.symmetric.encrypt_and_hash(payload)?;

        let mut msg = Vec::with_capacity(32 + encrypted_payload.len());
        msg.extend_from_slice(&e_pub);
        msg.extend_from_slice(&encrypted_payload);
        Ok(msg)
    }

    /// Responder reads message 1.
    pub fn read_message_1(&mut self, message: &[u8]) -> Result<Vec<u8>, ChannelError> {
        assert!(!self.initiator, "only responder reads message 1");

        if message.len() < 32 {
            return Err(ChannelError::Handshake("message 1 too short".into()));
        }

        // e token: read remote ephemeral
        let re_bytes = &message[..32];
        self.re = Some(Self::public_key_from_bytes(re_bytes)?);
        self.symmetric.mix_hash(re_bytes)?;

        // decrypt payload
        let payload = self.symmetric.decrypt_and_hash(&message[32..])?;
        Ok(payload)
    }

    // ── Message 2: <- e, ee, s, es ──

    /// Responder writes message 2.
    pub fn write_message_2(&mut self, payload: &[u8]) -> Result<Vec<u8>, ChannelError> {
        assert!(!self.initiator, "only responder writes message 2");

        // e token: generate ephemeral, mix_hash(e.public)
        let e = openssl_x25519_generate()?;
        let e_pub = Self::public_key_bytes(&e)?;
        self.symmetric.mix_hash(&e_pub)?;
        self.e = Some(e);

        // ee token: DH(e, re)
        let ee = Self::dh(
            self.e.as_ref().unwrap(),
            self.re
                .as_ref()
                .ok_or_else(|| ChannelError::Handshake("remote ephemeral not set".into()))?,
        )?;
        self.symmetric.mix_key(&ee)?;

        // s token: encrypt_and_hash(s.public)
        let s_pub = Self::public_key_bytes(&self.s)?;
        let encrypted_s = self.symmetric.encrypt_and_hash(&s_pub)?;

        // es token: DH(s, re)
        let es = Self::dh(&self.s, self.re.as_ref().unwrap())?;
        self.symmetric.mix_key(&es)?;

        // payload
        let encrypted_payload = self.symmetric.encrypt_and_hash(payload)?;

        let mut msg = Vec::with_capacity(32 + encrypted_s.len() + encrypted_payload.len());
        msg.extend_from_slice(&e_pub);
        msg.extend_from_slice(&encrypted_s);
        msg.extend_from_slice(&encrypted_payload);
        Ok(msg)
    }

    /// Initiator reads message 2.
    pub fn read_message_2(&mut self, message: &[u8]) -> Result<Vec<u8>, ChannelError> {
        assert!(self.initiator, "only initiator reads message 2");

        // e token: first 32 bytes are remote ephemeral
        if message.len() < 32 {
            return Err(ChannelError::Handshake("message 2 too short".into()));
        }
        let re_bytes = &message[..32];
        self.re = Some(Self::public_key_from_bytes(re_bytes)?);
        self.symmetric.mix_hash(re_bytes)?;

        // ee token: DH(e, re)
        let ee = Self::dh(
            self.e
                .as_ref()
                .ok_or_else(|| ChannelError::Handshake("local ephemeral not set".into()))?,
            self.re.as_ref().unwrap(),
        )?;
        self.symmetric.mix_key(&ee)?;

        // s token: next 32+16 bytes are encrypted static key
        let s_offset = 32;
        let s_end = s_offset + 32 + 16; // 32 key bytes + 16 AEAD tag
        if message.len() < s_end {
            return Err(ChannelError::Handshake(
                "message 2 too short for static key".into(),
            ));
        }
        let rs_bytes = self.symmetric.decrypt_and_hash(&message[s_offset..s_end])?;
        self.rs = Some(Self::public_key_from_bytes(&rs_bytes)?);

        // es token: DH(e, rs) — from initiator's perspective, this is e,rs
        let es = Self::dh(self.e.as_ref().unwrap(), self.rs.as_ref().unwrap())?;
        self.symmetric.mix_key(&es)?;

        // payload
        let payload = self.symmetric.decrypt_and_hash(&message[s_end..])?;
        Ok(payload)
    }

    // ── Message 3: -> s, se ──

    /// Initiator writes message 3.
    pub fn write_message_3(&mut self, payload: &[u8]) -> Result<Vec<u8>, ChannelError> {
        assert!(self.initiator, "only initiator writes message 3");

        // s token: encrypt_and_hash(s.public)
        let s_pub = Self::public_key_bytes(&self.s)?;
        let encrypted_s = self.symmetric.encrypt_and_hash(&s_pub)?;

        // se token: DH(s, re)
        let se = Self::dh(
            &self.s,
            self.re
                .as_ref()
                .ok_or_else(|| ChannelError::Handshake("remote ephemeral not set".into()))?,
        )?;
        self.symmetric.mix_key(&se)?;

        // payload
        let encrypted_payload = self.symmetric.encrypt_and_hash(payload)?;

        let mut msg = Vec::with_capacity(encrypted_s.len() + encrypted_payload.len());
        msg.extend_from_slice(&encrypted_s);
        msg.extend_from_slice(&encrypted_payload);
        Ok(msg)
    }

    /// Responder reads message 3.
    pub fn read_message_3(&mut self, message: &[u8]) -> Result<Vec<u8>, ChannelError> {
        assert!(!self.initiator, "only responder reads message 3");

        // s token: first 32+16 bytes are encrypted static key
        let s_end = 32 + 16;
        if message.len() < s_end {
            return Err(ChannelError::Handshake("message 3 too short".into()));
        }
        let rs_bytes = self.symmetric.decrypt_and_hash(&message[..s_end])?;
        self.rs = Some(Self::public_key_from_bytes(&rs_bytes)?);

        // se token: DH(e, rs) — from responder's perspective
        let se = Self::dh(
            self.e
                .as_ref()
                .ok_or_else(|| ChannelError::Handshake("local ephemeral not set".into()))?,
            self.rs.as_ref().unwrap(),
        )?;
        self.symmetric.mix_key(&se)?;

        // payload
        let payload = self.symmetric.decrypt_and_hash(&message[s_end..])?;
        Ok(payload)
    }

    /// Finalize the handshake: split into transport state.
    ///
    /// Returns (TransportState, remote_static_pubkey, handshake_hash).
    pub fn finalize(self) -> Result<FinalizedHandshake, ChannelError> {
        let (c1, c2) = self.symmetric.split()?;
        let handshake_hash = self.symmetric.handshake_hash().to_vec();

        let remote_static = self
            .rs
            .as_ref()
            .map(Self::public_key_bytes_pub)
            .transpose()?;

        let transport = if self.initiator {
            TransportState::new(c1, c2)
        } else {
            TransportState::new(c2, c1)
        };

        Ok((transport, remote_static, handshake_hash))
    }
}

/// Perform a full Noise_XX handshake between initiator and responder
/// using in-memory message passing (no I/O).
///
/// Returns (initiator_transport, responder_transport).
pub fn handshake_xx(
    initiator_static: PKey<Private>,
    responder_static: PKey<Private>,
) -> Result<(TransportState, TransportState), ChannelError> {
    let mut initiator = HandshakeState::new(initiator_static, true)?;
    let mut responder = HandshakeState::new(responder_static, false)?;

    // Message 1: initiator -> responder
    let msg1 = initiator.write_message_1(&[])?;
    responder.read_message_1(&msg1)?;

    // Message 2: responder -> initiator
    let msg2 = responder.write_message_2(&[])?;
    initiator.read_message_2(&msg2)?;

    // Message 3: initiator -> responder
    let msg3 = initiator.write_message_3(&[])?;
    responder.read_message_3(&msg3)?;

    // Finalize
    let (i_transport, _, _) = initiator.finalize()?;
    let (r_transport, _, _) = responder.finalize()?;

    Ok((i_transport, r_transport))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_handshake() {
        let i_static = openssl_x25519_generate().unwrap();
        let r_static = openssl_x25519_generate().unwrap();

        let (mut i_transport, mut r_transport) = handshake_xx(i_static, r_static).unwrap();

        // Initiator sends to responder
        let ct = i_transport.send(b"hello from initiator").unwrap();
        let pt = r_transport.recv(&ct).unwrap();
        assert_eq!(pt, b"hello from initiator");

        // Responder sends to initiator
        let ct = r_transport.send(b"hello from responder").unwrap();
        let pt = i_transport.recv(&ct).unwrap();
        assert_eq!(pt, b"hello from responder");
    }

    #[test]
    fn test_handshake_with_payload() {
        let i_static = openssl_x25519_generate().unwrap();
        let r_static = openssl_x25519_generate().unwrap();

        let mut initiator = HandshakeState::new(i_static, true).unwrap();
        let mut responder = HandshakeState::new(r_static, false).unwrap();

        let msg1 = initiator.write_message_1(b"init payload 1").unwrap();
        let p1 = responder.read_message_1(&msg1).unwrap();
        assert_eq!(p1, b"init payload 1");

        let msg2 = responder.write_message_2(b"resp payload 2").unwrap();
        let p2 = initiator.read_message_2(&msg2).unwrap();
        assert_eq!(p2, b"resp payload 2");

        let msg3 = initiator.write_message_3(b"init payload 3").unwrap();
        let p3 = responder.read_message_3(&msg3).unwrap();
        assert_eq!(p3, b"init payload 3");

        let (mut i_t, _, i_h) = initiator.finalize().unwrap();
        let (mut r_t, _, r_h) = responder.finalize().unwrap();

        // Handshake hashes should match
        assert_eq!(i_h, r_h);

        // Transport should work
        let ct = i_t.send(b"post-handshake").unwrap();
        let pt = r_t.recv(&ct).unwrap();
        assert_eq!(pt, b"post-handshake");
    }

    #[test]
    fn test_mutual_authentication() {
        let i_static = openssl_x25519_generate().unwrap();
        let r_static = openssl_x25519_generate().unwrap();

        let i_pub = i_static.raw_public_key().unwrap();
        let r_pub = r_static.raw_public_key().unwrap();

        let mut initiator = HandshakeState::new(i_static, true).unwrap();
        let mut responder = HandshakeState::new(r_static, false).unwrap();

        let msg1 = initiator.write_message_1(&[]).unwrap();
        responder.read_message_1(&msg1).unwrap();

        let msg2 = responder.write_message_2(&[]).unwrap();
        initiator.read_message_2(&msg2).unwrap();

        let msg3 = initiator.write_message_3(&[]).unwrap();
        responder.read_message_3(&msg3).unwrap();

        let (_, i_remote_static, _) = initiator.finalize().unwrap();
        let (_, r_remote_static, _) = responder.finalize().unwrap();

        // Initiator sees responder's static key
        assert_eq!(i_remote_static.unwrap(), r_pub);
        // Responder sees initiator's static key
        assert_eq!(r_remote_static.unwrap(), i_pub);
    }

    #[test]
    fn test_different_keys_different_transport() {
        let i1 = openssl_x25519_generate().unwrap();
        let r1 = openssl_x25519_generate().unwrap();
        let i2 = openssl_x25519_generate().unwrap();
        let r2 = openssl_x25519_generate().unwrap();

        let (mut t1_i, _) = handshake_xx(i1, r1).unwrap();
        let (mut t2_i, _) = handshake_xx(i2, r2).unwrap();

        // Same plaintext encrypted under different session keys -> different ciphertext
        let ct1 = t1_i.send(b"test").unwrap();
        let ct2 = t2_i.send(b"test").unwrap();
        assert_ne!(ct1, ct2);
    }
}

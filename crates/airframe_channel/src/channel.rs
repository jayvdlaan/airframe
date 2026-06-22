use crate::error::ChannelError;

/// Core trait for sending and receiving encrypted messages over a channel.
#[cfg(any(feature = "tcp", feature = "uds"))]
#[async_trait::async_trait]
pub trait Channel: Send {
    /// Send an encrypted message.
    async fn send(&mut self, payload: &[u8]) -> Result<(), ChannelError>;

    /// Receive and decrypt a message.
    async fn recv(&mut self) -> Result<Vec<u8>, ChannelError>;
}

/// NoiseSession wraps a transport (read/write halves) with Noise encryption.
///
/// After a Noise handshake completes, this struct provides encrypted
/// bidirectional communication over any async byte stream.
#[cfg(any(feature = "tcp", feature = "uds"))]
pub struct NoiseSession<R, W> {
    reader: R,
    writer: W,
    transport: crate::noise::TransportState,
}

#[cfg(any(feature = "tcp", feature = "uds"))]
impl<R, W> NoiseSession<R, W>
where
    R: tokio::io::AsyncReadExt + Unpin + Send,
    W: tokio::io::AsyncWriteExt + Unpin + Send,
{
    pub fn new(reader: R, writer: W, transport: crate::noise::TransportState) -> Self {
        Self {
            reader,
            writer,
            transport,
        }
    }
}

#[cfg(any(feature = "tcp", feature = "uds"))]
#[async_trait::async_trait]
impl<R, W> Channel for NoiseSession<R, W>
where
    R: tokio::io::AsyncReadExt + Unpin + Send,
    W: tokio::io::AsyncWriteExt + Unpin + Send,
{
    async fn send(&mut self, payload: &[u8]) -> Result<(), ChannelError> {
        let ciphertext = self.transport.send(payload)?;
        crate::framing::async_framing::write_frame(&mut self.writer, &ciphertext).await
    }

    async fn recv(&mut self) -> Result<Vec<u8>, ChannelError> {
        let ciphertext = crate::framing::async_framing::read_frame(&mut self.reader).await?;
        self.transport.recv(&ciphertext)
    }
}

/// Perform a full Noise_XX handshake over async read/write halves.
///
/// Returns a NoiseSession ready for encrypted communication.
#[cfg(any(feature = "tcp", feature = "uds"))]
pub async fn handshake_initiator<R, W>(
    mut reader: R,
    mut writer: W,
    local_static: openssl::pkey::PKey<openssl::pkey::Private>,
) -> Result<NoiseSession<R, W>, ChannelError>
where
    R: tokio::io::AsyncReadExt + Unpin + Send,
    W: tokio::io::AsyncWriteExt + Unpin + Send,
{
    use crate::framing::async_framing::{read_frame, write_frame};

    let mut hs = crate::noise::HandshakeState::new(local_static, true)?;

    // Message 1: -> e
    let msg1 = hs.write_message_1(&[])?;
    write_frame(&mut writer, &msg1).await?;

    // Message 2: <- e, ee, s, es
    let msg2 = read_frame(&mut reader).await?;
    hs.read_message_2(&msg2)?;

    // Message 3: -> s, se
    let msg3 = hs.write_message_3(&[])?;
    write_frame(&mut writer, &msg3).await?;

    let (transport, _, _) = hs.finalize()?;
    Ok(NoiseSession::new(reader, writer, transport))
}

/// Perform a full Noise_XX handshake as the responder.
#[cfg(any(feature = "tcp", feature = "uds"))]
pub async fn handshake_responder<R, W>(
    mut reader: R,
    mut writer: W,
    local_static: openssl::pkey::PKey<openssl::pkey::Private>,
) -> Result<NoiseSession<R, W>, ChannelError>
where
    R: tokio::io::AsyncReadExt + Unpin + Send,
    W: tokio::io::AsyncWriteExt + Unpin + Send,
{
    use crate::framing::async_framing::{read_frame, write_frame};

    let mut hs = crate::noise::HandshakeState::new(local_static, false)?;

    // Message 1: -> e
    let msg1 = read_frame(&mut reader).await?;
    hs.read_message_1(&msg1)?;

    // Message 2: <- e, ee, s, es
    let msg2 = hs.write_message_2(&[])?;
    write_frame(&mut writer, &msg2).await?;

    // Message 3: -> s, se
    let msg3 = read_frame(&mut reader).await?;
    hs.read_message_3(&msg3)?;

    let (transport, _, _) = hs.finalize()?;
    Ok(NoiseSession::new(reader, writer, transport))
}

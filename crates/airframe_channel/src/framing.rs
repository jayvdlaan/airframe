use crate::error::ChannelError;

/// Maximum Noise message size (65535 bytes per spec).
pub const MAX_MESSAGE_SIZE: usize = 65535;

/// Write a length-prefixed frame: 4-byte LE length + payload.
///
/// This is the sync framing helper that produces the byte vector.
pub fn frame_message(payload: &[u8]) -> Result<Vec<u8>, ChannelError> {
    if payload.len() > MAX_MESSAGE_SIZE {
        return Err(ChannelError::MessageTooLarge {
            size: payload.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }
    let len = payload.len() as u32;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Parse a length prefix from 4 bytes (LE u32).
pub fn parse_length(header: &[u8; 4]) -> Result<usize, ChannelError> {
    let len = u32::from_le_bytes(*header) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(ChannelError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }
    Ok(len)
}

/// Async framing helpers (require tokio).
#[cfg(any(feature = "tcp", feature = "uds", test))]
pub mod async_framing {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Write a length-prefixed frame to an async writer.
    pub async fn write_frame<W: AsyncWriteExt + Unpin>(
        writer: &mut W,
        payload: &[u8],
    ) -> Result<(), ChannelError> {
        if payload.len() > MAX_MESSAGE_SIZE {
            return Err(ChannelError::MessageTooLarge {
                size: payload.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }
        let len = payload.len() as u32;
        writer.write_all(&len.to_le_bytes()).await?;
        writer.write_all(payload).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Read a length-prefixed frame from an async reader.
    pub async fn read_frame<R: AsyncReadExt + Unpin>(
        reader: &mut R,
    ) -> Result<Vec<u8>, ChannelError> {
        let mut header = [0u8; 4];
        reader.read_exact(&mut header).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                ChannelError::UnexpectedEof
            } else {
                ChannelError::Io(e)
            }
        })?;
        let len = parse_length(&header)?;
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                ChannelError::UnexpectedEof
            } else {
                ChannelError::Io(e)
            }
        })?;
        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_message_roundtrip() {
        let payload = b"hello framing";
        let frame = frame_message(payload).unwrap();
        assert_eq!(frame.len(), 4 + payload.len());

        let mut header = [0u8; 4];
        header.copy_from_slice(&frame[..4]);
        let len = parse_length(&header).unwrap();
        assert_eq!(len, payload.len());
        assert_eq!(&frame[4..], payload);
    }

    #[test]
    fn test_frame_empty() {
        let frame = frame_message(b"").unwrap();
        assert_eq!(frame, &[0, 0, 0, 0]);
    }

    #[test]
    fn test_frame_too_large() {
        let payload = vec![0u8; MAX_MESSAGE_SIZE + 1];
        assert!(frame_message(&payload).is_err());
    }

    #[test]
    fn test_parse_length_max() {
        // Just at the limit
        let header = (MAX_MESSAGE_SIZE as u32).to_le_bytes();
        assert_eq!(parse_length(&header).unwrap(), MAX_MESSAGE_SIZE);

        // Over the limit
        let header = ((MAX_MESSAGE_SIZE + 1) as u32).to_le_bytes();
        assert!(parse_length(&header).is_err());
    }

    #[tokio::test]
    async fn test_async_framing_roundtrip() {
        use async_framing::*;

        let payload = b"async framing test";
        let mut buf = Vec::new();
        write_frame(&mut buf, payload).await.unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let result = read_frame(&mut cursor).await.unwrap();
        assert_eq!(result, payload);
    }

    #[tokio::test]
    async fn test_async_framing_multiple() {
        use async_framing::*;

        let messages = vec![b"msg1".to_vec(), b"msg2".to_vec(), b"msg3".to_vec()];
        let mut buf = Vec::new();
        for msg in &messages {
            write_frame(&mut buf, msg).await.unwrap();
        }

        let mut cursor = std::io::Cursor::new(buf);
        for msg in &messages {
            let result = read_frame(&mut cursor).await.unwrap();
            assert_eq!(result, *msg);
        }
    }

    #[tokio::test]
    async fn test_async_framing_various_sizes() {
        use async_framing::*;

        for size in [0, 1, 255, 256, 1024, 65535] {
            let payload = vec![0xAB; size];
            let mut buf = Vec::new();
            write_frame(&mut buf, &payload).await.unwrap();

            let mut cursor = std::io::Cursor::new(buf);
            let result = read_frame(&mut cursor).await.unwrap();
            assert_eq!(result, payload);
        }
    }
}

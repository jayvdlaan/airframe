use crate::channel::{handshake_initiator, handshake_responder, NoiseSession};
use crate::error::ChannelError;
use openssl::pkey::{PKey, Private};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;

/// A Noise-encrypted channel over Unix domain sockets.
pub type UdsChannel = NoiseSession<OwnedReadHalf, OwnedWriteHalf>;

/// Perform a Noise_XX handshake as the initiator over a Unix domain socket.
pub async fn uds_initiator(
    stream: UnixStream,
    local_static: PKey<Private>,
) -> Result<UdsChannel, ChannelError> {
    let (reader, writer) = stream.into_split();
    handshake_initiator(reader, writer, local_static).await
}

/// Perform a Noise_XX handshake as the responder over a Unix domain socket.
pub async fn uds_responder(
    stream: UnixStream,
    local_static: PKey<Private>,
) -> Result<UdsChannel, ChannelError> {
    let (reader, writer) = stream.into_split();
    handshake_responder(reader, writer, local_static).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::Channel;
    use airframe_crypt::asym::openssl_x25519_generate;
    use tokio::net::UnixListener;

    fn temp_sock_path(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "airframe_channel_test_{}_{}",
            name,
            std::process::id()
        ));
        // Clean up any stale socket
        let _ = std::fs::remove_file(&path);
        path
    }

    #[tokio::test]
    async fn test_uds_handshake_and_exchange() {
        let sock_path = temp_sock_path("basic");

        let listener = UnixListener::bind(&sock_path).unwrap();

        let server_key = openssl_x25519_generate().unwrap();
        let client_key = openssl_x25519_generate().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = uds_responder(stream, server_key).await.unwrap();
            let msg = session.recv().await.unwrap();
            assert_eq!(msg, b"hello over uds");
            session.send(b"ack over uds").await.unwrap();
        });

        let sock_path_clone = sock_path.clone();
        let client = tokio::spawn(async move {
            let stream = UnixStream::connect(&sock_path_clone).await.unwrap();
            let mut session = uds_initiator(stream, client_key).await.unwrap();
            session.send(b"hello over uds").await.unwrap();
            let msg = session.recv().await.unwrap();
            assert_eq!(msg, b"ack over uds");
        });

        server.await.unwrap();
        client.await.unwrap();
        let _ = std::fs::remove_file(&sock_path);
    }

    #[tokio::test]
    async fn test_uds_multiple_messages() {
        let sock_path = temp_sock_path("multi");

        let listener = UnixListener::bind(&sock_path).unwrap();

        let server_key = openssl_x25519_generate().unwrap();
        let client_key = openssl_x25519_generate().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = uds_responder(stream, server_key).await.unwrap();
            for i in 0..50 {
                let msg = session.recv().await.unwrap();
                assert_eq!(msg, format!("uds {}", i).as_bytes());
                session
                    .send(format!("uds ack {}", i).as_bytes())
                    .await
                    .unwrap();
            }
        });

        let sock_path_clone = sock_path.clone();
        let client = tokio::spawn(async move {
            let stream = UnixStream::connect(&sock_path_clone).await.unwrap();
            let mut session = uds_initiator(stream, client_key).await.unwrap();
            for i in 0..50 {
                session.send(format!("uds {}", i).as_bytes()).await.unwrap();
                let ack = session.recv().await.unwrap();
                assert_eq!(ack, format!("uds ack {}", i).as_bytes());
            }
        });

        server.await.unwrap();
        client.await.unwrap();
    }
}

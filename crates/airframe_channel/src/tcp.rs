use crate::channel::{handshake_initiator, handshake_responder, NoiseSession};
use crate::error::ChannelError;
use openssl::pkey::{PKey, Private};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

/// A Noise-encrypted channel over TCP.
pub type TcpChannel = NoiseSession<OwnedReadHalf, OwnedWriteHalf>;

/// Perform a Noise_XX handshake as the initiator over a TCP stream.
pub async fn tcp_initiator(
    stream: TcpStream,
    local_static: PKey<Private>,
) -> Result<TcpChannel, ChannelError> {
    let (reader, writer) = stream.into_split();
    handshake_initiator(reader, writer, local_static).await
}

/// Perform a Noise_XX handshake as the responder over a TCP stream.
pub async fn tcp_responder(
    stream: TcpStream,
    local_static: PKey<Private>,
) -> Result<TcpChannel, ChannelError> {
    let (reader, writer) = stream.into_split();
    handshake_responder(reader, writer, local_static).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::Channel;
    use airframe_crypt::asym::openssl_x25519_generate;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_tcp_handshake_and_exchange() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_key = openssl_x25519_generate().unwrap();
        let client_key = openssl_x25519_generate().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = tcp_responder(stream, server_key).await.unwrap();
            let msg = session.recv().await.unwrap();
            assert_eq!(msg, b"hello server");
            session.send(b"hello client").await.unwrap();
        });

        let client = tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let mut session = tcp_initiator(stream, client_key).await.unwrap();
            session.send(b"hello server").await.unwrap();
            let msg = session.recv().await.unwrap();
            assert_eq!(msg, b"hello client");
        });

        server.await.unwrap();
        client.await.unwrap();
    }

    #[tokio::test]
    async fn test_tcp_multiple_messages() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_key = openssl_x25519_generate().unwrap();
        let client_key = openssl_x25519_generate().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = tcp_responder(stream, server_key).await.unwrap();
            for i in 0..50 {
                let msg = session.recv().await.unwrap();
                assert_eq!(msg, format!("msg {}", i).as_bytes());
                session.send(format!("ack {}", i).as_bytes()).await.unwrap();
            }
        });

        let client = tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let mut session = tcp_initiator(stream, client_key).await.unwrap();
            for i in 0..50 {
                session.send(format!("msg {}", i).as_bytes()).await.unwrap();
                let ack = session.recv().await.unwrap();
                assert_eq!(ack, format!("ack {}", i).as_bytes());
            }
        });

        server.await.unwrap();
        client.await.unwrap();
    }
}

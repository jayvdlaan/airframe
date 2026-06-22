use crate::NetError;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

/// Maximum transmission unit for UDP payloads (conservative for internet).
pub const MAX_UDP_PAYLOAD: usize = 1200;

pub struct UdpTransport {
    socket: Arc<UdpSocket>,
    recv_buf: Vec<u8>,
}

impl UdpTransport {
    /// Bind to a local address.
    pub async fn bind(addr: SocketAddr) -> Result<Self, NetError> {
        let socket = UdpSocket::bind(addr).await?;
        Ok(Self {
            socket: Arc::new(socket),
            recv_buf: vec![0u8; MAX_UDP_PAYLOAD],
        })
    }

    /// Send raw bytes to a peer.
    pub async fn send_to(&self, data: &[u8], addr: SocketAddr) -> Result<usize, NetError> {
        let n = self.socket.send_to(data, addr).await?;
        Ok(n)
    }

    /// Receive raw bytes. Returns (data, sender_addr).
    pub async fn recv_from(&mut self) -> Result<(Vec<u8>, SocketAddr), NetError> {
        let (n, addr) = self.socket.recv_from(&mut self.recv_buf).await?;
        Ok((self.recv_buf[..n].to_vec(), addr))
    }

    /// Get the local address this transport is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, NetError> {
        let addr = self.socket.local_addr()?;
        Ok(addr)
    }

    /// Get a shared handle to the underlying socket for direct sends.
    ///
    /// This allows bypassing the outbound channel for latency-critical
    /// packets like keepalive echoes.
    pub fn send_handle(&self) -> Arc<UdpSocket> {
        Arc::clone(&self.socket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn udp_echo() {
        let addr_a: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let addr_b: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let transport_a = UdpTransport::bind(addr_a).await.unwrap();
        let mut transport_b = UdpTransport::bind(addr_b).await.unwrap();

        let bound_a = transport_a.local_addr().unwrap();
        let bound_b = transport_b.local_addr().unwrap();

        let payload = b"hello airframe";
        transport_a.send_to(payload, bound_b).await.unwrap();

        let (data, from) = transport_b.recv_from().await.unwrap();
        assert_eq!(&data, payload);
        assert_eq!(from, bound_a);
    }
}

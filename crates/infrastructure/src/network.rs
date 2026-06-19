// ── UDP Network Transport ─────────────────────
//  Implements `NetworkTransport` over tokio UDP
//  sockets with configurable receive deadlines.
//
//  The trait's `recv` method is blocking, so we
//  wrap tokio's async recv with `block_on`.

use gaming_application::traits::{NetworkTransport, MAX_UDP_PAYLOAD};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{Duration, Instant};

/// Maximum datagram size we will accept.
pub const MAX_DATAGRAM: usize = MAX_UDP_PAYLOAD;

pub struct UdpTransport {
    socket: UdpSocket,
    recv_deadline: Instant,
    peer_addr: Option<SocketAddr>,
}

impl UdpTransport {
    pub async fn bind(addr: impl tokio::net::ToSocketAddrs) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        Ok(Self {
            socket,
            recv_deadline: Instant::now() + Duration::from_secs(30),
            peer_addr: None,
        })
    }

    pub async fn connect(addr: impl tokio::net::ToSocketAddrs) -> std::io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(addr).await?;
        let peer = socket.peer_addr().ok();
        Ok(Self {
            socket,
            recv_deadline: Instant::now() + Duration::from_secs(30),
            peer_addr: peer,
        })
    }

    pub async fn bind_and_connect(
        bind_addr: impl tokio::net::ToSocketAddrs,
        remote_addr: impl tokio::net::ToSocketAddrs,
    ) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        socket.connect(remote_addr).await?;
        let peer = socket.peer_addr().ok();
        Ok(Self {
            socket,
            recv_deadline: Instant::now() + Duration::from_secs(30),
            peer_addr: peer,
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
    }
}

impl NetworkTransport for UdpTransport {
    fn send(&mut self, buf: &[u8]) -> Result<(), ()> {
        if buf.len() > MAX_DATAGRAM {
            return Err(());
        }
        self.socket.try_send(buf).map_err(|_| ())?;
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<(usize, u64), ()> {
        let deadline = self.recv_deadline;
        let now = Instant::now();
        let timeout = if deadline > now {
            deadline - now
        } else {
            Duration::ZERO
        };

        let fut = self.socket.recv_from(buf);
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                tokio::time::timeout(timeout, fut).await
            })
        });

        match result {
            Ok(Ok((n, addr))) => {
                // Convert SocketAddr to a u64 discriminator.
                let peer_id = match addr {
                    SocketAddr::V4(v4) => u64::from(v4.ip().to_bits()),
                    SocketAddr::V6(v6) => v6
                        .ip()
                        .to_ipv4_mapped()
                        .map_or(0, |ip| u64::from(ip.to_bits())),
                } | (u64::from(addr.port()) << 32);
                Ok((n, peer_id))
            }
            _ => Err(()),
        }
    }

    fn set_recv_deadline(&mut self, deadline: Instant) {
        self.recv_deadline = deadline;
    }
}

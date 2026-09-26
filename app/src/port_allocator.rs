//! Section 2.2 of the tunnel-subsystem change: hands a `ManagedForward` supervisor
//! (section 2.3) a local `127.0.0.1` port to listen on, either auto-allocated by the OS
//! or an explicit port the user configured for a tunnel.

use std::io;
use std::net::{SocketAddr, TcpListener};

/// A bound `127.0.0.1` listener plus the port it ended up on. Kept as a `TcpListener`
/// (not just the port number) so the caller can hand the same OS-level reservation
/// straight to its forward implementation without a race against another process
/// grabbing the port between allocation and use.
// UNWIRED(#3): the per-forward supervisor (section 2.3) allocates one of these per
// ManagedForward before spawning its ssh/port-forward process.
#[allow(dead_code)]
#[derive(Debug)]
pub struct AllocatedPort {
    listener: TcpListener,
    addr: SocketAddr,
}

impl AllocatedPort {
    #[allow(dead_code)]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[allow(dead_code)]
    pub fn into_listener(self) -> TcpListener {
        self.listener
    }
}

/// Binds `127.0.0.1:0`, letting the OS choose a free port.
// UNWIRED(#3): see the note on `AllocatedPort`.
#[allow(dead_code)]
pub fn allocate() -> io::Result<AllocatedPort> {
    bind_on("127.0.0.1:0")
}

/// Binds `127.0.0.1:<port>`. Fails with the underlying `AddrInUse` error (or whatever
/// the OS reports) if the port is already taken - the caller decides how to surface
/// that as a tunnel-config error.
// UNWIRED(#3): see the note on `AllocatedPort`.
#[allow(dead_code)]
pub fn allocate_explicit(port: u16) -> io::Result<AllocatedPort> {
    bind_on(("127.0.0.1", port))
}

fn bind_on(addr: impl std::net::ToSocketAddrs) -> io::Result<AllocatedPort> {
    let listener = TcpListener::bind(addr)?;
    let addr = listener.local_addr()?;
    Ok(AllocatedPort { listener, addr })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_allocation_returns_a_bound_local_port() {
        let allocated = allocate().unwrap();
        assert_eq!(allocated.addr().ip().to_string(), "127.0.0.1");
        assert_ne!(allocated.addr().port(), 0);
    }

    #[test]
    fn two_auto_allocations_get_different_ports() {
        let first = allocate().unwrap();
        let second = allocate().unwrap();
        assert_ne!(first.addr().port(), second.addr().port());
    }

    #[test]
    fn explicit_port_is_honored() {
        // Reserve a free port the OS picked, then re-request it explicitly to avoid
        // a fixed port number flaking on a busy CI box.
        let probe = allocate().unwrap();
        let port = probe.addr().port();
        drop(probe);

        let allocated = allocate_explicit(port).unwrap();
        assert_eq!(allocated.addr().port(), port);
    }

    #[test]
    fn explicit_port_already_in_use_reports_a_conflict() {
        let held = allocate().unwrap();
        let port = held.addr().port();

        let result = allocate_explicit(port);

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AddrInUse);
    }
}

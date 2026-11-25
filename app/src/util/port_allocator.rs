//! A free local port for a forward to listen on (tunnel-subsystem section 2.2).
//!
//! The OS picks a free `127.0.0.1` port, and the probe listener is dropped before the
//! address is returned: the ssh and command transports start a process that binds the
//! port itself, so there is no listener to hand over. Another process can take the port
//! in that gap; the forward then fails to bind and its supervisor retries.

use std::io;
use std::net::{SocketAddr, TcpListener};

/// A `127.0.0.1` address with a port the OS reported free a moment ago.
pub fn free_local_addr() -> io::Result<SocketAddr> {
    TcpListener::bind("127.0.0.1:0")?.local_addr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_is_local_with_a_real_port() {
        let addr = free_local_addr().unwrap();
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_ne!(addr.port(), 0);
    }

    #[test]
    fn the_port_is_free_to_bind() {
        // A parallel test can claim the port in between, as a real forward could;
        // retry that rather than report it as this module's failure.
        for _ in 0..8 {
            let addr = free_local_addr().unwrap();
            match TcpListener::bind(addr) {
                Ok(_) => return,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
                Err(error) => panic!("binding {addr} failed: {error}"),
            }
        }
        panic!("no free port could be bound in 8 attempts");
    }
}

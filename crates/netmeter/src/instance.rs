//! Guards against running two instances at once.
//!
//! Two copies would each sample the same OS counters and *both* write the same
//! deltas to the database, silently doubling every recorded number. Holding a
//! loopback port for the life of the process is enough to prevent that, and the
//! OS frees the port on exit, so a crash never leaves a stale lock behind.

use anyhow::{bail, Result};
use std::net::TcpListener;

/// Arbitrary, in the dynamic/private range.
const LOCK_PORT: u16 = 47_653;

pub struct InstanceGuard {
    _listener: TcpListener,
}

pub fn acquire() -> Result<InstanceGuard> {
    match TcpListener::bind(("127.0.0.1", LOCK_PORT)) {
        Ok(listener) => Ok(InstanceGuard {
            _listener: listener,
        }),
        Err(_) => bail!(
            "another NetMeter instance is already running (loopback port {LOCK_PORT} is in use)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_rejected_while_the_first_is_alive() {
        let first = acquire();
        // The port may already be taken on a busy machine; only assert when we got it.
        if let Ok(_guard) = first {
            assert!(acquire().is_err(), "a second instance should be rejected");
        }
    }
}

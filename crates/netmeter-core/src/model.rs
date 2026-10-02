use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign};

/// A pair of byte counters (received / transmitted).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Traffic {
    pub rx: u64,
    pub tx: u64,
}

impl Traffic {
    pub const ZERO: Traffic = Traffic { rx: 0, tx: 0 };

    pub const fn new(rx: u64, tx: u64) -> Self {
        Self { rx, tx }
    }

    /// Combined volume, saturating instead of overflowing.
    pub const fn total(self) -> u64 {
        self.rx.saturating_add(self.tx)
    }

    pub const fn is_zero(self) -> bool {
        self.rx == 0 && self.tx == 0
    }
}

impl Add for Traffic {
    type Output = Traffic;

    fn add(self, other: Traffic) -> Traffic {
        Traffic {
            rx: self.rx.saturating_add(other.rx),
            tx: self.tx.saturating_add(other.tx),
        }
    }
}

impl AddAssign for Traffic {
    fn add_assign(&mut self, other: Traffic) {
        *self = *self + other;
    }
}

/// An instantaneous transfer rate in bytes per second.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rate {
    pub rx_per_sec: f64,
    pub tx_per_sec: f64,
}

impl Rate {
    pub fn total_per_sec(self) -> f64 {
        self.rx_per_sec + self.tx_per_sec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_add_is_saturating() {
        let a = Traffic::new(u64::MAX, 0);
        let b = Traffic::new(10, 10);
        assert_eq!((a + b).rx, u64::MAX);
        assert_eq!((a + b).tx, 10);
    }

    #[test]
    fn traffic_total_combines_both_directions() {
        assert_eq!(Traffic::new(3, 4).total(), 7);
    }
}

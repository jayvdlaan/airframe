use crate::NetError;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

impl ConnectionState {
    /// Check if a transition is valid.
    pub fn can_transition_to(&self, next: ConnectionState) -> bool {
        matches!(
            (self, next),
            (Self::Disconnected, Self::Connecting)
                | (Self::Connecting, Self::Connected)
                | (Self::Connecting, Self::Disconnected)
                | (Self::Connected, Self::Disconnecting)
                | (Self::Disconnecting, Self::Disconnected)
                | (Self::Connected, Self::Disconnected)
        )
    }
}

/// Tracks connection lifecycle.
pub struct Connection {
    state: ConnectionState,
    last_received: Instant,
    last_sent: Instant,
}

impl Connection {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            state: ConnectionState::Disconnected,
            last_received: now,
            last_sent: now,
        }
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn transition(&mut self, next: ConnectionState) -> Result<(), NetError> {
        if self.state.can_transition_to(next) {
            self.state = next;
            Ok(())
        } else {
            Err(NetError::ConnectionRefused(format!(
                "invalid state transition: {:?} -> {:?}",
                self.state, next
            )))
        }
    }

    pub fn touch_received(&mut self) {
        self.last_received = Instant::now();
    }

    pub fn touch_sent(&mut self) {
        self.last_sent = Instant::now();
    }

    pub fn time_since_last_received(&self) -> Duration {
        self.last_received.elapsed()
    }

    pub fn time_since_last_sent(&self) -> Duration {
        self.last_sent.elapsed()
    }
}

impl Default for Connection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_transitions() {
        let mut conn = Connection::new();
        assert_eq!(conn.state(), ConnectionState::Disconnected);

        conn.transition(ConnectionState::Connecting).unwrap();
        assert_eq!(conn.state(), ConnectionState::Connecting);

        conn.transition(ConnectionState::Connected).unwrap();
        assert_eq!(conn.state(), ConnectionState::Connected);

        conn.transition(ConnectionState::Disconnecting).unwrap();
        assert_eq!(conn.state(), ConnectionState::Disconnecting);

        conn.transition(ConnectionState::Disconnected).unwrap();
        assert_eq!(conn.state(), ConnectionState::Disconnected);
    }

    #[test]
    fn connect_failure_transition() {
        let mut conn = Connection::new();
        conn.transition(ConnectionState::Connecting).unwrap();
        conn.transition(ConnectionState::Disconnected).unwrap();
        assert_eq!(conn.state(), ConnectionState::Disconnected);
    }

    #[test]
    fn abrupt_disconnect() {
        let mut conn = Connection::new();
        conn.transition(ConnectionState::Connecting).unwrap();
        conn.transition(ConnectionState::Connected).unwrap();
        conn.transition(ConnectionState::Disconnected).unwrap();
        assert_eq!(conn.state(), ConnectionState::Disconnected);
    }

    #[test]
    fn invalid_transition_returns_error() {
        let mut conn = Connection::new();
        assert!(conn.transition(ConnectionState::Connected).is_err());
        assert!(conn.transition(ConnectionState::Disconnecting).is_err());
    }

    #[test]
    fn invalid_transition_from_connecting() {
        let mut conn = Connection::new();
        conn.transition(ConnectionState::Connecting).unwrap();
        assert!(conn.transition(ConnectionState::Disconnecting).is_err());
    }
}

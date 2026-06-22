use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Configuration for the reliable channel.
pub struct ReliableConfig {
    pub max_retries: u32,
    pub retry_interval: Duration,
    pub ack_timeout: Duration,
}

impl Default for ReliableConfig {
    fn default() -> Self {
        Self {
            max_retries: 10,
            retry_interval: Duration::from_millis(200),
            ack_timeout: Duration::from_secs(5),
        }
    }
}

/// Reliability header size in bytes: u16 seq + u16 ack_seq + u32 ack_bitfield = 8 bytes.
const RELIABILITY_HEADER_SIZE: usize = 8;

/// A message pending acknowledgment.
struct PendingMessage {
    sequence: u16,
    data: Vec<u8>,
    sent_at: Instant,
    retries: u32,
}

/// Reliability layer: sequencing, acking, retransmission.
pub struct ReliableChannel {
    config: ReliableConfig,
    // Sending state
    next_send_seq: u16,
    pending_acks: VecDeque<PendingMessage>,
    // Receiving state
    next_expected_seq: u16,
    received_bitmap: u32,
}

impl ReliableChannel {
    pub fn new(config: ReliableConfig) -> Self {
        Self {
            config,
            next_send_seq: 0,
            pending_acks: VecDeque::new(),
            next_expected_seq: 0,
            received_bitmap: 0,
        }
    }

    /// Queue a message for reliable delivery. Returns (sequence_number, framed_data)
    /// where framed_data includes the reliability header (seq + ack + ack_bitfield).
    pub fn send(&mut self, data: Vec<u8>) -> (u16, Vec<u8>) {
        let seq = self.next_send_seq;
        self.next_send_seq = self.next_send_seq.wrapping_add(1);

        let framed = self.frame(seq, &data);

        self.pending_acks.push_back(PendingMessage {
            sequence: seq,
            data: framed.clone(),
            sent_at: Instant::now(),
            retries: 0,
        });

        (seq, framed)
    }

    /// Build a framed message with the reliability header prepended.
    fn frame(&self, seq: u16, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(RELIABILITY_HEADER_SIZE + payload.len());
        buf.extend_from_slice(&seq.to_be_bytes());
        // ack info: last seq we received from the other side
        let ack_seq = self.next_expected_seq.wrapping_sub(1);
        buf.extend_from_slice(&ack_seq.to_be_bytes());
        buf.extend_from_slice(&self.received_bitmap.to_be_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    /// Process an incoming reliable message. Returns the payload if this is a
    /// new, in-order message (or None if duplicate/out-of-order).
    /// Also extracts ack info from the header to clear pending messages.
    pub fn receive(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        if data.len() < RELIABILITY_HEADER_SIZE {
            return None;
        }

        let seq = u16::from_be_bytes([data[0], data[1]]);
        let ack_seq = u16::from_be_bytes([data[2], data[3]]);
        let ack_bitfield = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let payload = data[RELIABILITY_HEADER_SIZE..].to_vec();

        // Process ack information to clear our pending sends
        self.process_acks(ack_seq, ack_bitfield);

        // Check if this is the sequence we expect
        if seq == self.next_expected_seq {
            self.next_expected_seq = self.next_expected_seq.wrapping_add(1);
            // Shift bitmap left and set bit 0 for this new message
            self.received_bitmap = (self.received_bitmap << 1) | 1;
            Some(payload)
        } else {
            // Check if this is a recent sequence we can record in our bitmap
            let diff = self.next_expected_seq.wrapping_sub(seq);
            if diff > 0 && diff <= 32 {
                // Already received or past — mark in bitmap if not already
                self.received_bitmap |= 1 << (diff - 1);
            }
            None
        }
    }

    /// Process ack information from a received packet.
    fn process_acks(&mut self, ack_seq: u16, ack_bitfield: u32) {
        self.pending_acks.retain(|msg| {
            if msg.sequence == ack_seq {
                return false; // acked
            }
            let diff = ack_seq.wrapping_sub(msg.sequence);
            if diff > 0 && diff <= 32 && ack_bitfield & (1 << (diff - 1)) != 0 {
                return false; // acked via bitfield
            }
            true
        });
    }

    /// Get messages that need retransmission (exceeded retry interval).
    pub fn get_retransmissions(&mut self) -> Vec<Vec<u8>> {
        let now = Instant::now();
        let mut retransmissions = Vec::new();

        for msg in &mut self.pending_acks {
            if msg.retries < self.config.max_retries
                && now.duration_since(msg.sent_at) >= self.config.retry_interval
            {
                msg.retries += 1;
                msg.sent_at = now;
                retransmissions.push(msg.data.clone());
            }
        }

        retransmissions
    }

    /// Check for messages that have exceeded max retries. Returns their sequence
    /// numbers and removes them from the pending queue.
    pub fn get_timeouts(&mut self) -> Vec<u16> {
        let mut timed_out = Vec::new();

        self.pending_acks.retain(|msg| {
            if msg.retries >= self.config.max_retries {
                timed_out.push(msg.sequence);
                false
            } else {
                true
            }
        });

        timed_out
    }

    /// Get current pending (unacked) count.
    pub fn pending_count(&self) -> usize {
        self.pending_acks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_and_receive_in_order() {
        let mut sender = ReliableChannel::new(ReliableConfig::default());
        let mut receiver = ReliableChannel::new(ReliableConfig::default());

        let (seq0, framed0) = sender.send(b"hello".to_vec());
        let (seq1, framed1) = sender.send(b"world".to_vec());

        assert_eq!(seq0, 0);
        assert_eq!(seq1, 1);

        let payload0 = receiver.receive(&framed0).unwrap();
        assert_eq!(payload0, b"hello");

        let payload1 = receiver.receive(&framed1).unwrap();
        assert_eq!(payload1, b"world");
    }

    #[test]
    fn duplicate_is_ignored() {
        let mut sender = ReliableChannel::new(ReliableConfig::default());
        let mut receiver = ReliableChannel::new(ReliableConfig::default());

        let (_seq, framed) = sender.send(b"once".to_vec());

        let first = receiver.receive(&framed);
        assert!(first.is_some());

        let duplicate = receiver.receive(&framed);
        assert!(duplicate.is_none());
    }

    #[test]
    fn ack_processing_clears_pending() {
        let mut sender = ReliableChannel::new(ReliableConfig::default());
        let mut receiver = ReliableChannel::new(ReliableConfig::default());

        let (_seq, framed) = sender.send(b"ping".to_vec());
        assert_eq!(sender.pending_count(), 1);

        // Receiver processes the message
        receiver.receive(&framed);

        // Receiver sends a response (which carries ack info)
        let (_rseq, response) = receiver.send(b"pong".to_vec());

        // Sender receives the response, which acks the original message
        sender.receive(&response);
        assert_eq!(sender.pending_count(), 0);
    }

    #[test]
    fn retransmission_of_unacked() {
        let config = ReliableConfig {
            max_retries: 3,
            retry_interval: Duration::from_millis(0), // immediate for testing
            ack_timeout: Duration::from_secs(5),
        };
        let mut sender = ReliableChannel::new(config);

        sender.send(b"important".to_vec());
        assert_eq!(sender.pending_count(), 1);

        let retransmissions = sender.get_retransmissions();
        assert_eq!(retransmissions.len(), 1);
    }

    #[test]
    fn timeout_detection() {
        let config = ReliableConfig {
            max_retries: 1,
            retry_interval: Duration::from_millis(0),
            ack_timeout: Duration::from_secs(5),
        };
        let mut sender = ReliableChannel::new(config);

        let (seq, _framed) = sender.send(b"will timeout".to_vec());

        // First call to get_retransmissions bumps retries to 1 (== max)
        sender.get_retransmissions();

        // Now get_timeouts should detect the timed-out message
        let timeouts = sender.get_timeouts();
        assert_eq!(timeouts, vec![seq]);
        assert_eq!(sender.pending_count(), 0);
    }

    #[test]
    fn sequence_wrapping() {
        let mut sender = ReliableChannel::new(ReliableConfig::default());

        // Manually set next_send_seq near wrap point
        sender.next_send_seq = u16::MAX;

        let (seq0, _) = sender.send(b"wrap".to_vec());
        let (seq1, _) = sender.send(b"around".to_vec());

        assert_eq!(seq0, u16::MAX);
        assert_eq!(seq1, 0);
    }
}

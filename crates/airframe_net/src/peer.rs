use std::time::{Duration, Instant};

/// Number of recent RTT samples to keep in the sliding window.
const RTT_WINDOW_SIZE: usize = 8;

/// Network statistics for a peer connection.
pub struct PeerStats {
    rtt: Duration,
    rtt_samples: [Duration; RTT_WINDOW_SIZE],
    rtt_write_idx: usize,
    rtt_count: usize,
    packets_sent: u64,
    packets_received: u64,
    packets_lost: u64,
    bytes_sent: u64,
    bytes_received: u64,
    created_at: Instant,
    last_updated: Instant,
}

impl PeerStats {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            rtt: Duration::ZERO,
            rtt_samples: [Duration::ZERO; RTT_WINDOW_SIZE],
            rtt_write_idx: 0,
            rtt_count: 0,
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            bytes_sent: 0,
            bytes_received: 0,
            created_at: now,
            last_updated: now,
        }
    }

    /// Update RTT estimate with a new sample.
    ///
    /// Uses a sliding window of the last `RTT_WINDOW_SIZE` samples and
    /// reports the median.  This responds quickly to network changes while
    /// filtering single-sample spikes.
    pub fn update_rtt(&mut self, sample: Duration) {
        self.rtt_samples[self.rtt_write_idx] = sample;
        self.rtt_write_idx = (self.rtt_write_idx + 1) % RTT_WINDOW_SIZE;
        if self.rtt_count < RTT_WINDOW_SIZE {
            self.rtt_count += 1;
        }

        // Compute median of the filled portion.
        let n = self.rtt_count;
        let mut buf = [Duration::ZERO; RTT_WINDOW_SIZE];
        buf[..n].copy_from_slice(&self.rtt_samples[..n]);
        buf[..n].sort_unstable();
        self.rtt = if n % 2 == 1 {
            buf[n / 2]
        } else {
            (buf[n / 2 - 1] + buf[n / 2]) / 2
        };

        self.last_updated = Instant::now();
    }

    /// Record a sent packet.
    pub fn record_sent(&mut self, bytes: u64) {
        self.packets_sent += 1;
        self.bytes_sent += bytes;
        self.last_updated = Instant::now();
    }

    /// Record a received packet.
    pub fn record_received(&mut self, bytes: u64) {
        self.packets_received += 1;
        self.bytes_received += bytes;
        self.last_updated = Instant::now();
    }

    /// Record a lost packet.
    pub fn record_lost(&mut self) {
        self.packets_lost += 1;
        self.last_updated = Instant::now();
    }

    /// Get current RTT estimate.
    pub fn rtt(&self) -> Duration {
        self.rtt
    }

    /// Get packet loss ratio (0.0 - 1.0).
    pub fn loss_ratio(&self) -> f64 {
        let total = self.packets_sent + self.packets_lost;
        if total == 0 {
            0.0
        } else {
            self.packets_lost as f64 / total as f64
        }
    }

    /// Get bytes sent per second (average over connection lifetime).
    pub fn send_rate(&self) -> f64 {
        let elapsed = self.created_at.elapsed();
        if elapsed.as_millis() < 100 {
            return 0.0;
        }
        self.bytes_sent as f64 / elapsed.as_secs_f64()
    }

    /// Get bytes received per second (average over connection lifetime).
    pub fn recv_rate(&self) -> f64 {
        let elapsed = self.created_at.elapsed();
        if elapsed.as_millis() < 100 {
            return 0.0;
        }
        self.bytes_received as f64 / elapsed.as_secs_f64()
    }

    /// Get RTT jitter (standard deviation of recent samples) in microseconds.
    pub fn jitter_us(&self) -> u32 {
        if self.rtt_count < 2 {
            return 0;
        }
        let n = self.rtt_count;
        let mean_us = self.rtt_samples[..n]
            .iter()
            .map(|d| d.as_micros() as f64)
            .sum::<f64>()
            / n as f64;
        let variance = self.rtt_samples[..n]
            .iter()
            .map(|d| {
                let diff = d.as_micros() as f64 - mean_us;
                diff * diff
            })
            .sum::<f64>()
            / n as f64;
        variance.sqrt() as u32
    }
}

impl Default for PeerStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtt_tracks_samples() {
        let mut stats = PeerStats::new();

        // Fill window with 50ms samples.
        let target = Duration::from_millis(50);
        for _ in 0..8 {
            stats.update_rtt(target);
        }
        assert_eq!(stats.rtt(), target);

        // One spike shouldn't move the median much.
        stats.update_rtt(Duration::from_millis(500));
        assert!(
            stats.rtt() < Duration::from_millis(100),
            "median should resist single spike, got {:?}",
            stats.rtt()
        );

        // After filling with new value, median should match.
        let new_target = Duration::from_millis(30);
        for _ in 0..8 {
            stats.update_rtt(new_target);
        }
        assert_eq!(stats.rtt(), new_target);
    }

    #[test]
    fn loss_ratio_computation() {
        let mut stats = PeerStats::new();

        stats.record_sent(100);
        stats.record_sent(100);
        stats.record_sent(100);
        stats.record_lost();

        let ratio = stats.loss_ratio();
        assert!((ratio - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn initial_loss_ratio_is_zero() {
        let stats = PeerStats::new();
        assert_eq!(stats.loss_ratio(), 0.0);
    }

    #[test]
    fn record_received_updates_counter() {
        let mut stats = PeerStats::new();
        stats.record_received(512);
        stats.record_received(256);
        assert_eq!(stats.packets_received, 2);
        assert_eq!(stats.bytes_received, 768);
    }
}

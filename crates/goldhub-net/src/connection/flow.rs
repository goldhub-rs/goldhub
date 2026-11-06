//! Network flow control, bandwidth throttling, and RTT estimation.

/// Network flow statistics and metrics.
#[derive(Debug, Clone, Copy)]
pub struct FlowStats {
    pub srtt_ms: f32,
    pub rttvar_ms: f32,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub packets_dropped: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl Default for FlowStats {
    fn default() -> Self {
        Self {
            srtt_ms: 50.0, // Initial 50 ms estimate
            rttvar_ms: 10.0,
            packets_sent: 0,
            packets_received: 0,
            packets_dropped: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }
}

impl FlowStats {
    /// Updates Round Trip Time (RTT) estimation via RFC 6298 (Jacobson's algorithm).
    pub fn update_rtt(&mut self, sample_rtt_ms: f32) {
        if sample_rtt_ms <= 0.0 {
            return;
        }

        let diff = (self.srtt_ms - sample_rtt_ms).abs();
        self.rttvar_ms = 0.75 * self.rttvar_ms + 0.25 * diff;
        self.srtt_ms = 0.875 * self.srtt_ms + 0.125 * sample_rtt_ms;
    }

    /// Records dropped packets when an out-of-order gap in sequence numbers occurs.
    pub fn record_dropped_packets(&mut self, count: u64) {
        self.packets_dropped += count;
    }

    /// Computes the estimated packet loss percentage (0.0% to 100.0%).
    pub fn packet_loss_percentage(&self) -> f32 {
        let total = self.packets_received + self.packets_dropped;
        if total == 0 {
            0.0
        } else {
            (self.packets_dropped as f32 / total as f32) * 100.0
        }
    }
}

/// Token bucket rate limiter for client-side bandwidth pacing.
#[derive(Debug)]
pub struct RateLimiter {
    rate_bytes_per_sec: u32,
    available_tokens: f32,
    max_burst_bytes: f32,
}

impl RateLimiter {
    /// Initializes a rate limiter with the specified bytes/sec limit (e.g. 30,000 to 100,000).
    pub fn new(rate_bytes_per_sec: u32) -> Self {
        let max_burst_bytes = (rate_bytes_per_sec as f32) * 0.1; // 100 ms burst headroom
        Self {
            rate_bytes_per_sec,
            available_tokens: max_burst_bytes,
            max_burst_bytes,
        }
    }

    /// Adds tokens based on elapsed frame time `dt`.
    pub fn replenish(&mut self, dt: f32) {
        if dt > 0.0 {
            self.available_tokens = (self.available_tokens + (self.rate_bytes_per_sec as f32) * dt)
                .min(self.max_burst_bytes);
        }
    }

    /// Checks if a packet of `size_bytes` can be transmitted without violating flow control.
    pub fn try_consume(&mut self, size_bytes: usize) -> bool {
        let req = size_bytes as f32;
        if self.available_tokens >= req {
            self.available_tokens -= req;
            true
        } else {
            false
        }
    }

    /// Sets a new rate limit.
    pub fn set_rate(&mut self, rate_bytes_per_sec: u32) {
        self.rate_bytes_per_sec = rate_bytes_per_sec;
        self.max_burst_bytes = (rate_bytes_per_sec as f32) * 0.1;
    }
}

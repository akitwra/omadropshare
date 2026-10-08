use async_trait::async_trait;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel(u16);

impl Channel {
    pub fn new(number: u16) -> Result<Self, RadioError> {
        if matches!(number, 1..=233) {
            Ok(Self(number))
        } else {
            Err(RadioError::ChannelUnavailable(number))
        }
    }

    #[must_use]
    pub const fn number(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub monitor: bool,
    pub inject_action: Option<bool>,
    pub inject_data: Option<bool>,
    pub hardware_timestamps: Option<bool>,
    pub concurrent_station: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioFrame {
    pub bytes: Vec<u8>,
    pub monotonic_micros: u64,
    pub tsft_micros: Option<u64>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RadioError {
    #[error("radio permission denied")]
    PermissionDenied,
    #[error("adapter was removed")]
    AdapterRemoved,
    #[error("monitor mode is unavailable")]
    MonitorModeUnavailable,
    #[error("frame injection is unavailable")]
    InjectionUnavailable,
    #[error("channel {0} is unavailable")]
    ChannelUnavailable(u16),
    #[error("radio backend is not started")]
    NotStarted,
    #[error("no frame is available")]
    NoFrame,
}

#[async_trait]
pub trait RadioBackend: Send {
    async fn capabilities(&self) -> Result<BackendCapabilities, RadioError>;
    async fn prepare(&mut self) -> Result<(), RadioError>;
    async fn start(&mut self) -> Result<(), RadioError>;
    async fn stop(&mut self) -> Result<(), RadioError>;
    async fn set_channel(&mut self, channel: Channel) -> Result<(), RadioError>;
    async fn send_frame(&mut self, frame: &[u8]) -> Result<(), RadioError>;
    async fn recv_frame(&mut self) -> Result<RadioFrame, RadioError>;
}

#[derive(Debug)]
pub struct SimulatedRadioBackend {
    started: bool,
    channel: Channel,
    incoming: VecDeque<RadioFrame>,
    sent: Vec<Vec<u8>>,
}

impl Default for SimulatedRadioBackend {
    fn default() -> Self {
        Self {
            started: false,
            channel: Channel(44),
            incoming: VecDeque::new(),
            sent: Vec::new(),
        }
    }
}

impl SimulatedRadioBackend {
    pub fn push_incoming(&mut self, frame: RadioFrame) {
        self.incoming.push_back(frame);
    }

    #[must_use]
    pub fn sent_frames(&self) -> &[Vec<u8>] {
        &self.sent
    }

    #[must_use]
    pub const fn channel(&self) -> Channel {
        self.channel
    }
}

#[async_trait]
impl RadioBackend for SimulatedRadioBackend {
    async fn capabilities(&self) -> Result<BackendCapabilities, RadioError> {
        Ok(BackendCapabilities {
            monitor: true,
            inject_action: Some(true),
            inject_data: Some(true),
            hardware_timestamps: Some(true),
            concurrent_station: Some(true),
        })
    }

    async fn prepare(&mut self) -> Result<(), RadioError> {
        Ok(())
    }

    async fn start(&mut self) -> Result<(), RadioError> {
        self.started = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), RadioError> {
        self.started = false;
        Ok(())
    }

    async fn set_channel(&mut self, channel: Channel) -> Result<(), RadioError> {
        if !self.started {
            return Err(RadioError::NotStarted);
        }
        self.channel = channel;
        Ok(())
    }

    async fn send_frame(&mut self, frame: &[u8]) -> Result<(), RadioError> {
        if !self.started {
            return Err(RadioError::NotStarted);
        }
        self.sent.push(frame.to_vec());
        Ok(())
    }

    async fn recv_frame(&mut self) -> Result<RadioFrame, RadioError> {
        if !self.started {
            return Err(RadioError::NotStarted);
        }
        self.incoming.pop_front().ok_or(RadioError::NoFrame)
    }
}

#[derive(Debug, Default, Clone)]
pub struct TsftBridge {
    offset_micros: Option<i128>,
}

impl TsftBridge {
    pub fn observe(&mut self, tsft_micros: u64, host_monotonic_micros: u64) -> u64 {
        let observed = i128::from(host_monotonic_micros) - i128::from(tsft_micros);
        let offset = match self.offset_micros {
            None => observed,
            Some(previous) => {
                let delta = (observed - previous).clamp(-2_000, 2_000);
                previous + delta / 8
            }
        };
        self.offset_micros = Some(offset);
        apply_offset(tsft_micros, offset)
    }

    #[must_use]
    pub fn to_monotonic(&self, tsft_micros: u64) -> Option<u64> {
        self.offset_micros
            .map(|offset| apply_offset(tsft_micros, offset))
    }
}

fn apply_offset(value: u64, offset: i128) -> u64 {
    let adjusted = i128::from(value) + offset;
    u64::try_from(adjusted.max(0)).unwrap_or(u64::MAX)
}

#[must_use]
pub fn extend_wrapping_u32(previous_extended: u64, value: u32) -> u64 {
    let previous_low = previous_extended as u32;
    let mut high = previous_extended & !u64::from(u32::MAX);
    if value < previous_low && previous_low.wrapping_sub(value) > (u32::MAX / 2) {
        high = high.wrapping_add(1_u64 << 32);
    }
    high | u64::from(value)
}

#[must_use]
pub const fn availability_window(monotonic_micros: u64, period_tu: u64) -> u64 {
    if period_tu == 0 {
        return 0;
    }
    monotonic_micros / (period_tu * 1_024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn simulated_backend_requires_start() {
        let mut radio = SimulatedRadioBackend::default();
        assert_eq!(
            radio.send_frame(b"frame").await,
            Err(RadioError::NotStarted)
        );
        radio.start().await.unwrap();
        radio.send_frame(b"frame").await.unwrap();
        assert_eq!(radio.sent_frames(), &[b"frame".to_vec()]);
    }

    #[test]
    fn extends_timestamp_wraparound() {
        let previous = u64::from(u32::MAX) - 3;
        assert_eq!(extend_wrapping_u32(previous, 7), (1_u64 << 32) + 7);
    }

    #[test]
    fn tsft_offset_rejects_large_single_sample_jumps() {
        let mut bridge = TsftBridge::default();
        assert_eq!(bridge.observe(1_000, 11_000), 11_000);
        let translated = bridge.observe(2_000, 102_000);
        assert!(translated < 13_000);
    }

    #[test]
    fn availability_uses_monotonic_time() {
        assert_eq!(availability_window(16 * 1_024, 16), 1);
    }
}

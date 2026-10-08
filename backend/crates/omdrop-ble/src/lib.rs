use async_trait::async_trait;
use std::time::Duration;

pub const APPLE_COMPANY_ID: u16 = 0x004c;
pub const EVERYONE_WAKE_PAYLOAD: [u8; 20] = [
    0x05, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeAdvertisement {
    pub company_id: u16,
    pub payload: Vec<u8>,
    pub timeout: Duration,
}

impl WakeAdvertisement {
    #[must_use]
    pub fn everyone(requested_timeout: Duration) -> Self {
        Self {
            company_id: APPLE_COMPANY_ID,
            payload: EVERYONE_WAKE_PAYLOAD.to_vec(),
            timeout: requested_timeout.clamp(Duration::from_secs(1), Duration::from_secs(30)),
        }
    }

    pub fn validate(&self) -> Result<(), BleError> {
        if self.payload.len() > 24 {
            return Err(BleError::AdvertisementTooLarge(self.payload.len()));
        }
        if self.timeout > Duration::from_secs(30) {
            return Err(BleError::TimeoutTooLong);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BleError {
    #[error("no Bluetooth adapter is available")]
    AdapterUnavailable,
    #[error("Bluetooth is blocked or powered off")]
    PoweredOff,
    #[error("LE advertising is unavailable")]
    AdvertisingUnavailable,
    #[error("advertisement payload is {0} bytes; at most 24 bytes are allowed")]
    AdvertisementTooLarge(usize),
    #[error("wake advertisement may not run for more than 30 seconds")]
    TimeoutTooLong,
}

#[async_trait]
pub trait BleBackend: Send {
    async fn start_wake(&mut self, advertisement: &WakeAdvertisement) -> Result<(), BleError>;
    async fn stop_wake(&mut self) -> Result<(), BleError>;
}

#[derive(Debug, Default)]
pub struct SimulatedBleBackend {
    active: bool,
}

impl SimulatedBleBackend {
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }
}

#[async_trait]
impl BleBackend for SimulatedBleBackend {
    async fn start_wake(&mut self, advertisement: &WakeAdvertisement) -> Result<(), BleError> {
        advertisement.validate()?;
        self.active = true;
        Ok(())
    }

    async fn stop_wake(&mut self) -> Result<(), BleError> {
        self.active = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyone_payload_matches_verified_continuity_frame() {
        let advert = WakeAdvertisement::everyone(Duration::from_secs(15));
        assert_eq!(advert.company_id, 0x004c);
        assert_eq!(advert.payload.len(), 20);
        assert_eq!(&advert.payload[..2], &[0x05, 0x12]);
        assert!(advert.validate().is_ok());
    }

    #[test]
    fn timeout_is_bounded() {
        let advert = WakeAdvertisement::everyone(Duration::from_secs(120));
        assert_eq!(advert.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn simulated_backend_cleans_up() {
        let mut backend = SimulatedBleBackend::default();
        backend
            .start_wake(&WakeAdvertisement::everyone(Duration::from_secs(15)))
            .await
            .unwrap();
        assert!(backend.active());
        backend.stop_wake().await.unwrap();
        assert!(!backend.active());
    }
}

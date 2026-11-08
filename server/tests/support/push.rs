//! A push provider that records every wake ping and logs it like the logging
//! provider.

use std::sync::{Arc, Mutex, PoisonError};

use protocol::device_identity::DeviceId;
use server::pb as spb;
use server::push::{LoggingProvider, PushProvider};

/// The pings sent so far, shared with the test.
#[derive(Debug, Clone, Default)]
pub struct RecordingPush {
    sent: Arc<Mutex<Vec<(DeviceId, spb::PushToken)>>>,
}

impl RecordingPush {
    /// Every ping so far, oldest first.
    pub fn sent(&self) -> Vec<(DeviceId, spb::PushToken)> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Pings sent to `device_id` so far.
    pub fn count(&self, device_id: &DeviceId) -> usize {
        self.sent().iter().filter(|(id, _)| id == device_id).count()
    }
}

#[tonic::async_trait]
impl PushProvider for RecordingPush {
    async fn wake(&self, device_id: DeviceId, token: spb::PushToken) {
        LoggingProvider.wake(device_id, token.clone()).await;
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((device_id, token));
    }
}

//! A push provider that records every wake ping and logs it like the logging
//! provider. A test can make it answer "token invalid" for chosen tokens, or
//! pass each ping on to a real provider (pointed at fake FCM/APNs servers).

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use protocol::device_identity::DeviceId;
use server::pb as spb;
use server::push::{LoggingProvider, PushProvider, WakeOutcome};

/// The pings sent so far, shared with the test.
#[derive(Clone, Default)]
pub struct RecordingPush {
    sent: Arc<Mutex<Vec<(DeviceId, spb::PushToken)>>>,
    invalid: Arc<Mutex<HashSet<String>>>,
    forward: Arc<Mutex<Option<Arc<dyn PushProvider>>>>,
}

impl std::fmt::Debug for RecordingPush {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordingPush")
            .field("sent", &self.sent)
            .finish()
    }
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

    /// From now on, pings to `token` answer [`WakeOutcome::TokenInvalid`].
    pub fn answer_invalid(&self, token: &str) {
        self.invalid
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(token.to_owned());
    }

    /// From now on, every ping also goes to `provider`, whose answer counts.
    pub fn forward_to(&self, provider: Arc<dyn PushProvider>) {
        *self.forward.lock().unwrap_or_else(PoisonError::into_inner) = Some(provider);
    }
}

#[tonic::async_trait]
impl PushProvider for RecordingPush {
    async fn wake(&self, device_id: DeviceId, token: spb::PushToken) -> WakeOutcome {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((device_id, token.clone()));
        let forward = self
            .forward
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(provider) = forward {
            return provider.wake(device_id, token).await;
        }
        let invalid = self
            .invalid
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&token.token);
        LoggingProvider.wake(device_id, token).await;
        if invalid {
            WakeOutcome::TokenInvalid
        } else {
            WakeOutcome::Sent
        }
    }
}

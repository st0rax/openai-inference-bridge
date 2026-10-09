//! Project-owned browser Brain contract and narrow web-chat adapter.
//!
//! This module contains no WebAgent types or concrete WebView dependency.
//! A platform runtime implements BrowserPageDriver; the adapter owns one Brain turn.

use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::brain_registry::Brain;

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    LoginRequired,
    Challenge,
    RateLimited,
    Unknown,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendErrorKind {
    LoginRequired,
    Challenge,
    RateLimited,
    Timeout,
    Cancelled,
    BrowserUnavailable,
    NavigationFailed,
    SubmissionFailed,
    ResponseNotDetected,
    ExtractionFailed,
    UnsupportedCapability,
    ReadinessUnknown,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    pub kind: BackendErrorKind,
    /// Safe, non-sensitive diagnostic text. Never include prompt, profile path, or cookies.
    pub message: &'static str,
}

impl BackendError {
    pub fn new(kind: BackendErrorKind, message: &'static str) -> Self {
        Self { kind, message }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub prompt: String,
    pub deadline: Instant,
    pub cancellation: CancellationToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    AssistantTextSnapshot(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSnapshot {
    pub text: String,
    pub complete: bool,
}

/// Minimal browser controls needed by the first text-only Brain adapter.
///
/// A concrete driver must bound each operation by the supplied absolute deadline.
/// next_snapshot should block or wait for a bounded interval, not busy-spin.
pub trait BrowserPageDriver: Send {
    fn start(
        &mut self,
        profile_dir: &Path,
        start_url: &str,
        deadline: Instant,
    ) -> Result<(), BackendError>;

    fn readiness(&mut self, deadline: Instant) -> Result<Readiness, BackendError>;

    fn submit_prompt(&mut self, prompt: &str, deadline: Instant) -> Result<(), BackendError>;

    fn next_snapshot(
        &mut self,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Option<TextSnapshot>, BackendError>;

    fn shutdown(&mut self) -> Result<(), BackendError>;
}

pub trait BrainBackend {
    fn start(&mut self, deadline: Instant) -> Result<(), BackendError>;
    fn readiness(&mut self, deadline: Instant) -> Result<Readiness, BackendError>;
    fn infer(
        &mut self,
        request: &InferenceRequest,
        event_sink: &mut dyn FnMut(BackendEvent),
    ) -> Result<String, BackendError>;
    fn shutdown(&mut self) -> Result<(), BackendError>;
}

pub struct BrowserBrainBackend<D: BrowserPageDriver> {
    brain_id: String,
    start_url: String,
    profile_dir: PathBuf,
    driver: D,
    started: bool,
    shutdown_failed: bool,
}

impl<D: BrowserPageDriver> BrowserBrainBackend<D> {
    pub fn new(brain: &Brain, driver: D) -> Result<Self, BackendError> {
        if !brain.enabled {
            return Err(BackendError::new(
                BackendErrorKind::UnsupportedCapability,
                "disabled Brain cannot start a browser backend",
            ));
        }
        if brain.adapter_kind.as_deref() != Some("chatgpt-web") {
            return Err(BackendError::new(
                BackendErrorKind::UnsupportedCapability,
                "Brain adapter kind is not supported by this backend",
            ));
        }
        let start_url = brain.start_url.clone().ok_or_else(|| {
            BackendError::new(
                BackendErrorKind::UnsupportedCapability,
                "enabled Brain has no configured start URL",
            )
        })?;

        Ok(Self {
            brain_id: brain.id.clone(),
            start_url,
            profile_dir: brain.profile_dir.clone(),
            driver,
            started: false,
            shutdown_failed: false,
        })
    }

    fn check_deadline(deadline: Instant) -> Result<(), BackendError> {
        if Instant::now() >= deadline {
            Err(BackendError::new(
                BackendErrorKind::Timeout,
                "Brain operation deadline expired",
            ))
        } else {
            Ok(())
        }
    }

    fn require_started(&self) -> Result<(), BackendError> {
        if self.shutdown_failed {
            return Err(BackendError::new(
                BackendErrorKind::BrowserUnavailable,
                "previous browser shutdown failed; backend cannot be reused",
            ));
        }
        if !self.started {
            return Err(BackendError::new(
                BackendErrorKind::BrowserUnavailable,
                "browser backend has not been started",
            ));
        }
        Ok(())
    }

    fn readiness_error(readiness: Readiness) -> Option<BackendError> {
        let (kind, message) = match readiness {
            Readiness::Ready => return None,
            Readiness::LoginRequired => (
                BackendErrorKind::LoginRequired,
                "provider login is required",
            ),
            Readiness::Challenge => (
                BackendErrorKind::Challenge,
                "provider challenge detected",
            ),
            Readiness::RateLimited => (
                BackendErrorKind::RateLimited,
                "provider rate limit detected",
            ),
            Readiness::Unknown => (
                BackendErrorKind::ReadinessUnknown,
                "provider readiness could not be established",
            ),
            Readiness::Failed => (
                BackendErrorKind::BrowserUnavailable,
                "browser readiness probe failed",
            ),
        };
        Some(BackendError::new(kind, message))
    }

    pub fn brain_id(&self) -> &str {
        &self.brain_id
    }
}

impl<D: BrowserPageDriver> BrainBackend for BrowserBrainBackend<D> {
    fn start(&mut self, deadline: Instant) -> Result<(), BackendError> {
        Self::check_deadline(deadline)?;
        if self.started || self.shutdown_failed {
            return Err(BackendError::new(
                BackendErrorKind::Internal,
                "browser backend cannot be started in its current lifecycle state",
            ));
        }
        self.driver
            .start(&self.profile_dir, &self.start_url, deadline)?;
        self.started = true;
        Ok(())
    }

    fn readiness(&mut self, deadline: Instant) -> Result<Readiness, BackendError> {
        self.require_started()?;
        Self::check_deadline(deadline)?;
        self.driver.readiness(deadline)
    }

    fn infer(
        &mut self,
        request: &InferenceRequest,
        event_sink: &mut dyn FnMut(BackendEvent),
    ) -> Result<String, BackendError> {
        self.require_started()?;
        Self::check_deadline(request.deadline)?;
        if request.cancellation.is_cancelled() {
            return Err(BackendError::new(
                BackendErrorKind::Cancelled,
                "inference was cancelled",
            ));
        }

        let readiness = self.driver.readiness(request.deadline)?;
        if let Some(error) = Self::readiness_error(readiness) {
            return Err(error);
        }
        self.driver
            .submit_prompt(&request.prompt, request.deadline)?;

        loop {
            Self::check_deadline(request.deadline)?;
            if request.cancellation.is_cancelled() {
                return Err(BackendError::new(
                    BackendErrorKind::Cancelled,
                    "inference was cancelled",
                ));
            }
            let Some(snapshot) = self
                .driver
                .next_snapshot(request.deadline, &request.cancellation)?
            else {
                continue;
            };
            event_sink(BackendEvent::AssistantTextSnapshot(snapshot.text.clone()));
            if snapshot.complete {
                if snapshot.text.trim().is_empty() {
                    return Err(BackendError::new(
                        BackendErrorKind::ResponseNotDetected,
                        "provider completion was empty",
                    ));
                }
                return Ok(snapshot.text);
            }
        }
    }

    fn shutdown(&mut self) -> Result<(), BackendError> {
        if !self.started && !self.shutdown_failed {
            return Ok(());
        }
        match self.driver.shutdown() {
            Ok(()) => {
                self.started = false;
                self.shutdown_failed = false;
                Ok(())
            }
            Err(error) => {
                self.shutdown_failed = true;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::Duration;

    struct FakeDriver {
        readiness: Readiness,
        snapshots: VecDeque<TextSnapshot>,
        submitted: Option<String>,
        started: bool,
        shutdown: bool,
    }

    impl FakeDriver {
        fn ready_with(snapshots: Vec<TextSnapshot>) -> Self {
            Self {
                readiness: Readiness::Ready,
                snapshots: snapshots.into(),
                submitted: None,
                started: false,
                shutdown: false,
            }
        }
    }

    impl BrowserPageDriver for FakeDriver {
        fn start(
            &mut self,
            _profile_dir: &Path,
            _start_url: &str,
            _deadline: Instant,
        ) -> Result<(), BackendError> {
            self.started = true;
            Ok(())
        }

        fn readiness(&mut self, _deadline: Instant) -> Result<Readiness, BackendError> {
            Ok(self.readiness)
        }

        fn submit_prompt(&mut self, prompt: &str, _deadline: Instant) -> Result<(), BackendError> {
            self.submitted = Some(prompt.to_owned());
            Ok(())
        }

        fn next_snapshot(
            &mut self,
            _deadline: Instant,
            _cancellation: &CancellationToken,
        ) -> Result<Option<TextSnapshot>, BackendError> {
            Ok(self.snapshots.pop_front())
        }

        fn shutdown(&mut self) -> Result<(), BackendError> {
            self.shutdown = true;
            Ok(())
        }
    }

    fn brain() -> Brain {
        Brain {
            id: "chatgpt".to_owned(),
            display_name: "ChatGPT".to_owned(),
            enabled: true,
            start_url: Some("https://chatgpt.com/".to_owned()),
            adapter_kind: Some("chatgpt-web".to_owned()),
            profile_dir: PathBuf::from("isolated-profile"),
        }
    }

    fn request(cancellation: CancellationToken) -> InferenceRequest {
        InferenceRequest {
            prompt: "preserve all conversation context".to_owned(),
            deadline: Instant::now() + Duration::from_secs(2),
            cancellation,
        }
    }

    #[test]
    fn browser_adapter_emits_snapshots_and_returns_authoritative_final_text() {
        let driver = FakeDriver::ready_with(vec![
            TextSnapshot {
                text: "partial".to_owned(),
                complete: false,
            },
            TextSnapshot {
                text: "final answer".to_owned(),
                complete: true,
            },
        ]);
        let mut backend = BrowserBrainBackend::new(&brain(), driver).unwrap();
        backend.start(Instant::now() + Duration::from_secs(1)).unwrap();
        let request = request(CancellationToken::default());
        let mut events = Vec::new();
        let result = backend
            .infer(&request, &mut |event| events.push(event))
            .unwrap();

        assert_eq!(result, "final answer");
        assert_eq!(events.len(), 2);
        assert_eq!(
            backend.driver.submitted.as_deref(),
            Some("preserve all conversation context")
        );
        backend.shutdown().unwrap();
        assert!(backend.driver.shutdown);
    }

    #[test]
    fn readiness_failures_are_typed_and_not_guessed_as_ready() {
        let mut driver = FakeDriver::ready_with(Vec::new());
        driver.readiness = Readiness::LoginRequired;
        let mut backend = BrowserBrainBackend::new(&brain(), driver).unwrap();
        backend.start(Instant::now() + Duration::from_secs(1)).unwrap();

        let error = backend
            .infer(&request(CancellationToken::default()), &mut |_| {})
            .unwrap_err();
        assert_eq!(error.kind, BackendErrorKind::LoginRequired);
    }

    #[test]
    fn cancellation_and_deadline_are_checked_before_submission() {
        let mut backend =
            BrowserBrainBackend::new(&brain(), FakeDriver::ready_with(Vec::new())).unwrap();
        backend.start(Instant::now() + Duration::from_secs(1)).unwrap();

        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let error = backend
            .infer(&request(cancellation), &mut |_| {})
            .unwrap_err();
        assert_eq!(error.kind, BackendErrorKind::Cancelled);

        let expired = InferenceRequest {
            prompt: "unused".to_owned(),
            deadline: Instant::now() - Duration::from_millis(1),
            cancellation: CancellationToken::default(),
        };
        let error = backend.infer(&expired, &mut |_| {}).unwrap_err();
        assert_eq!(error.kind, BackendErrorKind::Timeout);
    }

    #[test]
    fn empty_final_snapshot_is_not_a_successful_completion() {
        let driver = FakeDriver::ready_with(vec![TextSnapshot {
            text: "  ".to_owned(),
            complete: true,
        }]);
        let mut backend = BrowserBrainBackend::new(&brain(), driver).unwrap();
        backend.start(Instant::now() + Duration::from_secs(1)).unwrap();
        let error = backend
            .infer(&request(CancellationToken::default()), &mut |_| {})
            .unwrap_err();
        assert_eq!(error.kind, BackendErrorKind::ResponseNotDetected);
    }

    #[test]
    fn disabled_brains_cannot_create_backends() {
        let mut disabled = brain();
        disabled.enabled = false;
        let result = BrowserBrainBackend::new(&disabled, FakeDriver::ready_with(Vec::new()));
        assert_eq!(
            result.unwrap_err().kind,
            BackendErrorKind::UnsupportedCapability
        );
    }
}

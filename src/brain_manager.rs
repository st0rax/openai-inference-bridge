//! Per-Brain worker ownership and bounded command lifecycle.

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender, SyncSender},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{
    brain_backend::{
        BackendError, BackendErrorKind, BackendEvent, BrainBackend, CancellationToken,
        InferenceRequest, Readiness,
    },
    brain_registry::{Brain, BrainRegistry},
};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);

type BackendFactory =
    dyn Fn(&Brain) -> Result<Box<dyn BrainBackend>, BackendError> + Send + Sync + 'static;

pub struct BrainManager {
    registry: BrainRegistry,
    factory: Arc<BackendFactory>,
    workers: Mutex<HashMap<String, WorkerHandle>>,
}

struct WorkerHandle {
    sender: Sender<Command>,
    poisoned: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

enum Command {
    Start {
        deadline: Instant,
        reply: SyncSender<Result<(), BackendError>>,
    },
    Readiness {
        deadline: Instant,
        reply: SyncSender<Result<Readiness, BackendError>>,
    },
    Infer {
        request: InferenceRequest,
        events: Sender<BackendEvent>,
        reply: SyncSender<Result<String, BackendError>>,
    },
    Shutdown {
        deadline: Instant,
        reply: SyncSender<Result<(), BackendError>>,
    },
}

impl BrainManager {
    pub fn new<F>(registry: BrainRegistry, factory: F) -> Self
    where
        F: Fn(&Brain) -> Result<Box<dyn BrainBackend>, BackendError> + Send + Sync + 'static,
    {
        Self {
            registry,
            factory: Arc::new(factory),
            workers: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, brain_id: &str, deadline: Instant) -> Result<(), BackendError> {
        let (sender, poisoned) = self.worker_for(brain_id)?;
        let (reply, receiver) = mpsc::sync_channel(1);
        sender
            .send(Command::Start { deadline, reply })
            .map_err(|_| {
                poisoned.store(true, Ordering::Release);
                unavailable_error()
            })?;
        receive(receiver, deadline, &poisoned)
    }

    pub fn readiness(
        &self,
        brain_id: &str,
        deadline: Instant,
    ) -> Result<Readiness, BackendError> {
        let (sender, poisoned) = self.worker_for(brain_id)?;
        let (reply, receiver) = mpsc::sync_channel(1);
        sender
            .send(Command::Readiness { deadline, reply })
            .map_err(|_| {
                poisoned.store(true, Ordering::Release);
                unavailable_error()
            })?;
        receive(receiver, deadline, &poisoned)
    }

    pub fn infer(
        &self,
        brain_id: &str,
        request: InferenceRequest,
        event_sink: &mut dyn FnMut(BackendEvent),
    ) -> Result<String, BackendError> {
        remaining(request.deadline)?;
        let (sender, poisoned) = self.worker_for(brain_id)?;
        let (events, event_receiver) = mpsc::channel();
        let (reply, receiver) = mpsc::sync_channel(1);
        sender
            .send(Command::Infer {
                request: request.clone(),
                events,
                reply,
            })
            .map_err(|_| {
                poisoned.store(true, Ordering::Release);
                unavailable_error()
            })?;

        loop {
            let wait = match remaining(request.deadline) {
                Ok(wait) => wait.min(EVENT_POLL_INTERVAL),
                Err(error) => {
                    poisoned.store(true, Ordering::Release);
                    return Err(error);
                }
            };
            match event_receiver.recv_timeout(wait) {
                Ok(event) => event_sink(event),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        receive(receiver, request.deadline, &poisoned)
    }

    pub fn shutdown(&self, brain_id: &str, deadline: Instant) -> Result<(), BackendError> {
        let (sender, poisoned) = {
            let workers = self.workers.lock().map_err(|_| {
                BackendError::new(BackendErrorKind::Internal, "worker registry lock failed")
            })?;
            let Some(worker) = workers.get(brain_id) else {
                return Ok(());
            };
            (worker.sender.clone(), Arc::clone(&worker.poisoned))
        };

        let (reply, receiver) = mpsc::sync_channel(1);
        if sender.send(Command::Shutdown { deadline, reply }).is_err() {
            poisoned.store(true, Ordering::Release);
            return Err(unavailable_error());
        }

        let remaining_time = match remaining(deadline) {
            Ok(value) => value,
            Err(error) => {
                poisoned.store(true, Ordering::Release);
                return Err(error);
            }
        };
        let result = match receiver.recv_timeout(remaining_time) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                poisoned.store(true, Ordering::Release);
                return Err(timeout_error());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                poisoned.store(true, Ordering::Release);
                return Err(unavailable_error());
            }
        };

        let handle = self
            .workers
            .lock()
            .map_err(|_| {
                BackendError::new(BackendErrorKind::Internal, "worker registry lock failed")
            })?
            .remove(brain_id);

        if let Some(mut handle) = handle {
            if let Some(join) = handle.join.take() {
                if join.join().is_err() {
                    return Err(BackendError::new(
                        BackendErrorKind::Internal,
                        "Brain worker thread panicked",
                    ));
                }
            }
        }
        result
    }

    pub fn shutdown_all(&self, deadline: Instant) -> Result<(), BackendError> {
        let brain_ids = self
            .workers
            .lock()
            .map_err(|_| {
                BackendError::new(BackendErrorKind::Internal, "worker registry lock failed")
            })?
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        let mut first_error = None;
        for brain_id in brain_ids {
            if let Err(error) = self.shutdown(&brain_id, deadline) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn worker_for(
        &self,
        brain_id: &str,
    ) -> Result<(Sender<Command>, Arc<AtomicBool>), BackendError> {
        let brain = self
            .registry
            .brains()
            .iter()
            .find(|brain| brain.id == brain_id && brain.enabled)
            .ok_or_else(|| {
                BackendError::new(
                    BackendErrorKind::UnsupportedCapability,
                    "Brain is not configured and enabled",
                )
            })?;

        let mut workers = self.workers.lock().map_err(|_| {
            BackendError::new(BackendErrorKind::Internal, "worker registry lock failed")
        })?;
        if let Some(worker) = workers.get(brain_id) {
            if worker.poisoned.load(Ordering::Acquire) {
                return Err(unavailable_error());
            }
            return Ok((worker.sender.clone(), Arc::clone(&worker.poisoned)));
        }

        let backend = (self.factory)(brain)?;
        let (sender, receiver) = mpsc::channel();
        let poisoned = Arc::new(AtomicBool::new(false));
        let worker_poisoned = Arc::clone(&poisoned);
        let thread_name = format!("oib-brain-{brain_id}");
        let join = thread::Builder::new()
            .name(thread_name)
            .spawn(move || worker_loop(backend, receiver, worker_poisoned))
            .map_err(|_| {
                BackendError::new(
                    BackendErrorKind::BrowserUnavailable,
                    "could not start Brain worker thread",
                )
            })?;

        workers.insert(
            brain_id.to_owned(),
            WorkerHandle {
                sender: sender.clone(),
                poisoned: Arc::clone(&poisoned),
                join: Some(join),
            },
        );
        Ok((sender, poisoned))
    }
}

impl Drop for BrainManager {
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_millis(500);
        let _ = self.shutdown_all(deadline);
    }
}

fn worker_loop(
    mut backend: Box<dyn BrainBackend>,
    receiver: Receiver<Command>,
    poisoned: Arc<AtomicBool>,
) {
    while let Ok(command) = receiver.recv() {
        if poisoned.load(Ordering::Acquire) && !matches!(&command, Command::Shutdown { .. }) {
            reject_command(command);
            continue;
        }

        match command {
            Command::Start { deadline, reply } => {
                let result = backend.start(deadline);
                if result.is_err() {
                    poisoned.store(true, Ordering::Release);
                }
                let _ = reply.send(result);
            }
            Command::Readiness { deadline, reply } => {
                let result = backend.readiness(deadline);
                if result
                    .as_ref()
                    .is_err_and(|error| should_poison(error.kind))
                {
                    poisoned.store(true, Ordering::Release);
                }
                let _ = reply.send(result);
            }
            Command::Infer {
                request,
                events,
                reply,
            } => {
                let result = backend.infer(&request, &mut |event| {
                    let _ = events.send(event);
                });
                if result
                    .as_ref()
                    .is_err_and(|error| should_poison(error.kind))
                {
                    poisoned.store(true, Ordering::Release);
                }
                let _ = reply.send(result);
            }
            Command::Shutdown { deadline, reply } => {
                let result = backend.shutdown(deadline);
                if result.is_err() {
                    poisoned.store(true, Ordering::Release);
                }
                let _ = reply.send(result);
                break;
            }
        }
    }
}

fn reject_command(command: Command) {
    let error = unavailable_error();
    match command {
        Command::Start { reply, .. } | Command::Shutdown { reply, .. } => {
            let _ = reply.send(Err(error));
        }
        Command::Readiness { reply, .. } => {
            let _ = reply.send(Err(error));
        }
        Command::Infer { reply, .. } => {
            let _ = reply.send(Err(error));
        }
    }
}

fn receive<T>(
    receiver: mpsc::Receiver<Result<T, BackendError>>,
    deadline: Instant,
    poisoned: &AtomicBool,
) -> Result<T, BackendError> {
    let wait = match remaining(deadline) {
        Ok(value) => value,
        Err(error) => {
            poisoned.store(true, Ordering::Release);
            return Err(error);
        }
    };
    match receiver.recv_timeout(wait) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            poisoned.store(true, Ordering::Release);
            Err(timeout_error())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            poisoned.store(true, Ordering::Release);
            Err(unavailable_error())
        }
    }
}

fn remaining(deadline: Instant) -> Result<Duration, BackendError> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err(timeout_error())
    } else {
        Ok(duration)
    }
}

fn timeout_error() -> BackendError {
    BackendError::new(BackendErrorKind::Timeout, "Brain operation deadline expired")
}

fn unavailable_error() -> BackendError {
    BackendError::new(
        BackendErrorKind::BrowserUnavailable,
        "Brain worker is unavailable or poisoned",
    )
}

fn should_poison(kind: BackendErrorKind) -> bool {
    matches!(
        kind,
        BackendErrorKind::Timeout
            | BackendErrorKind::Cancelled
            | BackendErrorKind::BrowserUnavailable
            | BackendErrorKind::NavigationFailed
            | BackendErrorKind::SubmissionFailed
            | BackendErrorKind::ResponseNotDetected
            | BackendErrorKind::ExtractionFailed
            | BackendErrorKind::Internal
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        brain_backend::{BackendEvent, TextSnapshot},
        config::Config,
    };
    use std::sync::atomic::AtomicUsize;
    use std::thread;
    use std::time::Duration;

    struct TestBackend {
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        barrier: Arc<Mutex<Option<Arc<std::sync::Barrier>>>>,
        delay: Duration,
    }

    impl BrainBackend for TestBackend {
        fn start(&mut self, _deadline: Instant) -> Result<(), BackendError> {
            Ok(())
        }

        fn readiness(&mut self, _deadline: Instant) -> Result<Readiness, BackendError> {
            Ok(Readiness::Ready)
        }

        fn infer(
            &mut self,
            request: &InferenceRequest,
            event_sink: &mut dyn FnMut(BackendEvent),
        ) -> Result<String, BackendError> {
            let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
            self.peak.fetch_max(active, Ordering::AcqRel);
            let barrier = self.barrier.lock().unwrap().clone();
            if let Some(barrier) = barrier {
                let _ = barrier.wait();
            }
            thread::sleep(self.delay);
            self.active.fetch_sub(1, Ordering::AcqRel);
            event_sink(BackendEvent::AssistantTextSnapshot(request.prompt.clone()));
            Ok(format!("reply:{}", request.prompt))
        }

        fn shutdown(&mut self, deadline: Instant) -> Result<(), BackendError> {
            remaining(deadline)?;
            Ok(())
        }
    }

    fn registry() -> BrainRegistry {
        let data_dir = std::env::temp_dir().to_string_lossy().into_owned();
        let config = Config::from_lookup(|key| match key {
            "OIB_API_TOKEN" => Some("manager-test-token-that-is-long-enough".to_owned()),
            "OIB_DATA_DIR" => Some(data_dir.clone()),
            _ => None,
        })
        .unwrap();
        BrainRegistry::from_lookup(&config, |key| match key {
            "OIB_BRAIN_IDS" => Some("chatgpt,other".to_owned()),
            "OIB_BRAIN_CHATGPT_ENABLED" | "OIB_BRAIN_OTHER_ENABLED" => Some("1".to_owned()),
            "OIB_BRAIN_CHATGPT_LABEL" => Some("ChatGPT".to_owned()),
            "OIB_BRAIN_CHATGPT_URL" => Some("https://chatgpt.com/".to_owned()),
            "OIB_BRAIN_CHATGPT_ADAPTER" => Some("chatgpt-web".to_owned()),
            "OIB_BRAIN_OTHER_LABEL" => Some("Other".to_owned()),
            "OIB_BRAIN_OTHER_URL" => Some("https://example.com/".to_owned()),
            "OIB_BRAIN_OTHER_ADAPTER" => Some("chatgpt-web".to_owned()),
            _ => None,
        })
        .unwrap()
    }

    fn request(prompt: &str) -> InferenceRequest {
        InferenceRequest {
            prompt: prompt.to_owned(),
            deadline: Instant::now() + Duration::from_secs(3),
            cancellation: CancellationToken::default(),
        }
    }

    #[test]
    fn same_brain_turns_are_serialized_and_different_brains_can_run_concurrently() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Mutex::new(None));
        let factory_active = Arc::clone(&active);
        let factory_peak = Arc::clone(&peak);
        let factory_barrier = Arc::clone(&barrier);
        let manager = Arc::new(BrainManager::new(registry(), move |_| {
            Ok(Box::new(TestBackend {
                active: Arc::clone(&factory_active),
                peak: Arc::clone(&factory_peak),
                barrier: Arc::clone(&factory_barrier),
                delay: Duration::from_millis(60),
            }) as Box<dyn BrainBackend>)
        }));

        let deadline = Instant::now() + Duration::from_secs(2);
        manager.start("chatgpt", deadline).unwrap();
        manager.start("other", deadline).unwrap();

        peak.store(0, Ordering::Release);
        let first = Arc::clone(&manager);
        let second = Arc::clone(&manager);
        let one = thread::spawn(move || first.infer("chatgpt", request("one"), &mut |_| {}).unwrap());
        let two = thread::spawn(move || second.infer("chatgpt", request("two"), &mut |_| {}).unwrap());
        one.join().unwrap();
        two.join().unwrap();
        assert_eq!(peak.load(Ordering::Acquire), 1);

        peak.store(0, Ordering::Release);
        *barrier.lock().unwrap() = Some(Arc::new(std::sync::Barrier::new(2)));
        let first = Arc::clone(&manager);
        let second = Arc::clone(&manager);
        let one = thread::spawn(move || first.infer("chatgpt", request("one"), &mut |_| {}).unwrap());
        let two = thread::spawn(move || second.infer("other", request("two"), &mut |_| {}).unwrap());
        one.join().unwrap();
        two.join().unwrap();
        assert_eq!(peak.load(Ordering::Acquire), 2);

        manager
            .shutdown_all(Instant::now() + Duration::from_secs(2))
            .unwrap();
    }

    #[test]
    fn unknown_or_disabled_brains_are_not_started() {
        let manager = BrainManager::new(registry(), |_| {
            Err(BackendError::new(
                BackendErrorKind::Internal,
                "factory should not run",
            ))
        });
        let error = manager
            .start("missing", Instant::now() + Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(error.kind, BackendErrorKind::UnsupportedCapability);
    }

    #[test]
    fn timed_out_worker_is_poisoned_and_not_silently_reused() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Mutex::new(None));
        let factory_active = Arc::clone(&active);
        let factory_peak = Arc::clone(&peak);
        let factory_barrier = Arc::clone(&barrier);
        let manager = BrainManager::new(registry(), move |_| {
            Ok(Box::new(TestBackend {
                active: Arc::clone(&factory_active),
                peak: Arc::clone(&factory_peak),
                barrier: Arc::clone(&factory_barrier),
                delay: Duration::from_millis(100),
            }) as Box<dyn BrainBackend>)
        });
        manager
            .start("chatgpt", Instant::now() + Duration::from_secs(1))
            .unwrap();

        let error = manager
            .infer(
                "chatgpt",
                InferenceRequest {
                    prompt: "slow".to_owned(),
                    deadline: Instant::now() + Duration::from_millis(10),
                    cancellation: CancellationToken::default(),
                },
                &mut |_| {},
            )
            .unwrap_err();
        assert_eq!(error.kind, BackendErrorKind::Timeout);

        let error = manager
            .readiness("chatgpt", Instant::now() + Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(error.kind, BackendErrorKind::BrowserUnavailable);
        manager
            .shutdown_all(Instant::now() + Duration::from_secs(2))
            .unwrap();
    }

    #[test]
    fn shutdown_is_explicit_and_bounded() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Mutex::new(None));
        let manager = BrainManager::new(registry(), move |_| {
            Ok(Box::new(TestBackend {
                active: Arc::clone(&active),
                peak: Arc::clone(&peak),
                barrier: Arc::clone(&barrier),
                delay: Duration::from_millis(1),
            }) as Box<dyn BrainBackend>)
        });
        manager
            .start("chatgpt", Instant::now() + Duration::from_secs(1))
            .unwrap();
        manager
            .shutdown("chatgpt", Instant::now() + Duration::from_secs(1))
            .unwrap();
        assert!(manager.start("chatgpt", Instant::now() + Duration::from_secs(1)).is_ok());
    }
}

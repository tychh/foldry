use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt,
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    Clock, ExecutionControl, Extensions, FoldryError, FoldryWarning, LogLevel, LogRecord,
    LogRepository, ProgressSnapshot, RepositoryError, ResultSummary, RunEvent, RunEventKind,
    RunHistoryRepository, RunId, RunOutcome, RunRecord, RunState,
};

pub trait RunExecutor: Send + Sync {
    fn execute(
        &self,
        run: &RunRecord,
        control: &ExecutionControl,
        reporter: &dyn RunReporter,
    ) -> ResultSummary;
}

pub trait RunReporter: Send + Sync {
    fn progress(&self, progress: ProgressSnapshot);
    fn warning(&self, warning: FoldryWarning);
    fn error(&self, error: FoldryError);
    fn log(&self, level: LogLevel, message: String, path: Option<String>);
}

pub trait RunEventSink: Send + Sync {
    fn publish(&self, event: RunEvent);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopRunEventSink;

impl RunEventSink for NoopRunEventSink {
    fn publish(&self, _event: RunEvent) {}
}

pub struct SchedulerPorts {
    pub history: Arc<dyn RunHistoryRepository>,
    pub logs: Arc<dyn LogRepository>,
    pub clock: Arc<dyn Clock>,
    pub executor: Arc<dyn RunExecutor>,
    pub events: Arc<dyn RunEventSink>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    InvalidLimit,
    DuplicateRun(RunId),
    RunNotFound(RunId),
    InvalidInitialState(RunState),
    InvalidTransition { from: RunState, to: RunState },
    Persistence(String),
    Background(String),
    Unavailable,
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit => {
                formatter.write_str("scheduler parallelism must be between 1 and 64")
            }
            Self::DuplicateRun(run_id) => write!(formatter, "run {run_id} is already scheduled"),
            Self::RunNotFound(run_id) => write!(formatter, "run {run_id} is not scheduled"),
            Self::InvalidInitialState(state) => {
                write!(
                    formatter,
                    "new scheduler run must be queued, received {state:?}"
                )
            }
            Self::InvalidTransition { from, to } => {
                write!(formatter, "invalid run transition {from:?} -> {to:?}")
            }
            Self::Persistence(message) => {
                write!(formatter, "scheduler persistence failed: {message}")
            }
            Self::Background(message) => write!(formatter, "scheduler worker failed: {message}"),
            Self::Unavailable => formatter.write_str("scheduler is shutting down"),
        }
    }
}

impl std::error::Error for SchedulerError {}

impl From<RepositoryError> for SchedulerError {
    fn from(error: RepositoryError) -> Self {
        Self::Persistence(error.message)
    }
}

pub struct Scheduler {
    inner: Arc<SchedulerInner>,
    dispatcher: Option<JoinHandle<()>>,
    event_dispatcher: Option<JoinHandle<()>>,
}

struct SchedulerInner {
    ports: SchedulerPorts,
    max_parallel_runs: AtomicUsize,
    progress_interval: Duration,
    event_sender: mpsc::Sender<EventMessage>,
    state: Mutex<SchedulerState>,
    wake: Condvar,
}

#[derive(Default)]
struct SchedulerState {
    queue: VecDeque<RunId>,
    runs: HashMap<RunId, ManagedRun>,
    active_slots: usize,
    globally_paused: bool,
    shutdown: bool,
    background_error: Option<String>,
}

struct ManagedRun {
    record: RunRecord,
    control: ExecutionControl,
    event_sequence: u64,
    log_sequence: u64,
    last_progress_event: Option<Instant>,
}

impl Scheduler {
    pub fn start(ports: SchedulerPorts, max_parallel_runs: u16) -> Result<Self, SchedulerError> {
        if !(1..=64).contains(&max_parallel_runs) {
            return Err(SchedulerError::InvalidLimit);
        }
        let (event_sender, event_receiver) = mpsc::channel();
        let event_sink = Arc::clone(&ports.events);
        let event_dispatcher = thread::Builder::new()
            .name("foldry-events".into())
            .spawn(move || dispatch_events(event_receiver, event_sink))
            .map_err(|error| SchedulerError::Background(error.to_string()))?;
        let inner = Arc::new(SchedulerInner {
            ports,
            max_parallel_runs: AtomicUsize::new(usize::from(max_parallel_runs)),
            progress_interval: Duration::from_millis(100),
            event_sender,
            state: Mutex::new(SchedulerState::default()),
            wake: Condvar::new(),
        });
        let dispatcher_inner = Arc::clone(&inner);
        let dispatcher = thread::Builder::new()
            .name("foldry-scheduler".into())
            .spawn(move || dispatch_loop(dispatcher_inner))
            .map_err(|error| SchedulerError::Background(error.to_string()))?;
        Ok(Self {
            inner,
            dispatcher: Some(dispatcher),
            event_dispatcher: Some(event_dispatcher),
        })
    }

    pub fn enqueue(&self, run: RunRecord) -> Result<(), SchedulerError> {
        if run.state != RunState::Queued {
            return Err(SchedulerError::InvalidInitialState(run.state));
        }
        self.check_health()?;
        let run_id = run.run_id;
        self.inner.ports.history.update(&run)?;
        let event = {
            let mut state = self.inner.lock_state()?;
            if state.shutdown {
                return Err(SchedulerError::Unavailable);
            }
            if state.runs.contains_key(&run_id) {
                return Err(SchedulerError::DuplicateRun(run_id));
            }
            let mut managed = ManagedRun {
                record: run,
                control: ExecutionControl::default(),
                event_sequence: 0,
                log_sequence: 0,
                last_progress_event: None,
            };
            let event = next_event(
                &mut managed,
                self.inner.ports.clock.now(),
                RunEventKind::StateChanged {
                    state: RunState::Queued,
                },
            );
            state.runs.insert(run_id, managed);
            state.queue.push_back(run_id);
            event
        };
        self.inner.publish_event(event);
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn record(&self, run_id: RunId) -> Result<RunRecord, SchedulerError> {
        self.check_health()?;
        self.inner
            .lock_state()?
            .runs
            .get(&run_id)
            .map(|run| run.record.clone())
            .ok_or(SchedulerError::RunNotFound(run_id))
    }

    pub fn records(&self) -> Result<Vec<RunRecord>, SchedulerError> {
        self.check_health()?;
        let mut records = self
            .inner
            .lock_state()?
            .runs
            .values()
            .map(|run| run.record.clone())
            .collect::<Vec<_>>();
        records.sort_by_key(|run| (run.started_at, run.run_id));
        Ok(records)
    }

    pub fn set_max_parallel_runs(&self, max_parallel_runs: u16) -> Result<(), SchedulerError> {
        if !(1..=64).contains(&max_parallel_runs) {
            return Err(SchedulerError::InvalidLimit);
        }
        self.check_health()?;
        self.inner
            .max_parallel_runs
            .store(usize::from(max_parallel_runs), Ordering::Release);
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn pause(&self, run_id: RunId) -> Result<bool, SchedulerError> {
        self.command_transition(run_id, Command::Pause)
    }

    pub fn resume(&self, run_id: RunId) -> Result<bool, SchedulerError> {
        self.command_transition(run_id, Command::Resume)
    }

    pub fn stop(&self, run_id: RunId) -> Result<bool, SchedulerError> {
        self.command_transition(run_id, Command::Stop)
    }

    pub fn pause_all(&self) -> Result<u64, SchedulerError> {
        {
            let mut state = self.inner.lock_state()?;
            state.globally_paused = true;
        }
        let run_ids = self.non_terminal_run_ids()?;
        let mut changed = 0;
        for run_id in run_ids {
            changed += u64::from(self.pause(run_id)?);
        }
        Ok(changed)
    }

    pub fn resume_all(&self) -> Result<u64, SchedulerError> {
        {
            let mut state = self.inner.lock_state()?;
            state.globally_paused = false;
        }
        let run_ids = self.non_terminal_run_ids()?;
        let mut changed = 0;
        for run_id in run_ids {
            changed += u64::from(self.resume(run_id)?);
        }
        self.inner.wake.notify_all();
        Ok(changed)
    }

    pub fn stop_all(&self) -> Result<u64, SchedulerError> {
        let run_ids = self.non_terminal_run_ids()?;
        let mut changed = 0;
        for run_id in run_ids {
            changed += u64::from(self.stop(run_id)?);
        }
        Ok(changed)
    }

    pub fn check_health(&self) -> Result<(), SchedulerError> {
        let state = self.inner.lock_state()?;
        match &state.background_error {
            Some(error) => Err(SchedulerError::Background(error.clone())),
            None if state.shutdown => Err(SchedulerError::Unavailable),
            None => Ok(()),
        }
    }

    fn non_terminal_run_ids(&self) -> Result<Vec<RunId>, SchedulerError> {
        Ok(self
            .inner
            .lock_state()?
            .runs
            .iter()
            .filter_map(|(run_id, run)| (!is_terminal(run.record.state)).then_some(*run_id))
            .collect())
    }

    fn command_transition(&self, run_id: RunId, command: Command) -> Result<bool, SchedulerError> {
        self.check_health()?;
        let (events, notify) = {
            let mut state = self.inner.lock_state()?;
            let current = state
                .runs
                .get(&run_id)
                .ok_or(SchedulerError::RunNotFound(run_id))?
                .record
                .state;
            let (next, notify) = match (command, current) {
                (Command::Pause, RunState::Planning | RunState::Running) => {
                    (Some(RunState::Paused), false)
                }
                (Command::Pause, RunState::Paused) => return Ok(false),
                (Command::Pause, _) => return Ok(false),
                (Command::Resume, RunState::Paused) => (Some(RunState::Running), true),
                (Command::Resume, RunState::Running) => return Ok(false),
                (Command::Resume, _) => return Ok(false),
                (Command::Stop, RunState::Queued) => (Some(RunState::Stopped), true),
                (Command::Stop, RunState::Planning | RunState::Running | RunState::Paused) => {
                    (Some(RunState::Stopping), true)
                }
                (Command::Stop, RunState::Stopping | RunState::Stopped) => return Ok(false),
                (Command::Stop, _) => return Ok(false),
            };
            let next = next.expect("changed command has a next state");
            validate_transition(current, next)?;
            if current == RunState::Queued {
                state.queue.retain(|candidate| *candidate != run_id);
            }
            let managed = state
                .runs
                .get_mut(&run_id)
                .expect("run still exists after queue update");
            match command {
                Command::Pause => managed.control.pause(),
                Command::Resume => managed.control.resume(),
                Command::Stop => managed.control.stop(),
            }
            managed.record.state = next;
            let state_event = next_event(
                managed,
                self.inner.ports.clock.now(),
                RunEventKind::StateChanged { state: next },
            );
            let mut events = vec![state_event];
            if next == RunState::Stopped {
                let summary = stopped_summary();
                managed.record.finished_at = Some(self.inner.ports.clock.now());
                managed.record.summary = Some(summary.clone());
                events.push(next_event(
                    managed,
                    self.inner.ports.clock.now(),
                    RunEventKind::Completed { summary },
                ));
            }
            self.inner.ports.history.update(&managed.record)?;
            (events, notify)
        };
        for event in events {
            self.inner.publish_event(event);
        }
        if notify {
            self.inner.wake.notify_all();
        }
        Ok(true)
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.shutdown = true;
            for run in state.runs.values() {
                if !is_terminal(run.record.state) {
                    run.control.stop();
                }
            }
        }
        self.inner.wake.notify_all();
        if let Some(dispatcher) = self.dispatcher.take() {
            let _ = dispatcher.join();
        }
        let _ = self.inner.event_sender.send(EventMessage::Shutdown);
        if let Some(dispatcher) = self.event_dispatcher.take() {
            let _ = dispatcher.join();
        }
    }
}

#[derive(Clone, Copy)]
enum Command {
    Pause,
    Resume,
    Stop,
}

impl SchedulerInner {
    fn lock_state(&self) -> Result<MutexGuard<'_, SchedulerState>, SchedulerError> {
        self.state.lock().map_err(|_| SchedulerError::Unavailable)
    }

    fn store_background_error(&self, error: impl Into<String>) {
        if let Ok(mut state) = self.state.lock() {
            state.background_error.get_or_insert_with(|| error.into());
        }
        self.wake.notify_all();
    }

    fn report_event(&self, run_id: RunId, kind: RunEventKind) {
        let event = self.state.lock().ok().and_then(|mut state| {
            state
                .runs
                .get_mut(&run_id)
                .map(|run| next_event(run, self.ports.clock.now(), kind))
        });
        if let Some(event) = event {
            self.publish_event(event);
        }
    }

    fn publish_event(&self, event: RunEvent) {
        if self
            .event_sender
            .send(EventMessage::Event(Box::new(event)))
            .is_err()
        {
            self.store_background_error("event dispatcher is unavailable");
        }
    }
}

enum EventMessage {
    Event(Box<RunEvent>),
    Shutdown,
}

fn dispatch_events(receiver: mpsc::Receiver<EventMessage>, sink: Arc<dyn RunEventSink>) {
    let mut expected = HashMap::<RunId, u64>::new();
    let mut pending = HashMap::<RunId, BTreeMap<u64, RunEvent>>::new();
    while let Ok(message) = receiver.recv() {
        match message {
            EventMessage::Event(event) => {
                let event = *event;
                let run_id = event.run_id;
                pending
                    .entry(run_id)
                    .or_default()
                    .insert(event.sequence, event);
                let next = expected.entry(run_id).or_default();
                while let Some(event) = pending
                    .get_mut(&run_id)
                    .and_then(|events| events.remove(next))
                {
                    sink.publish(event);
                    *next = next.saturating_add(1);
                }
            }
            EventMessage::Shutdown => break,
        }
    }
}

fn dispatch_loop(inner: Arc<SchedulerInner>) {
    loop {
        let dispatched = {
            let mut state = match inner.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            while !state.shutdown
                && (state.globally_paused
                    || state.queue.is_empty()
                    || state.active_slots >= inner.max_parallel_runs.load(Ordering::Acquire))
            {
                state = match inner.wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            if state.shutdown {
                return;
            }
            let run_id = state.queue.pop_front().expect("non-empty queue");
            state.active_slots += 1;
            let managed = state.runs.get_mut(&run_id).expect("queued run exists");
            let from = managed.record.state;
            validate_transition(from, RunState::Planning)
                .expect("only queued runs are kept in the dispatch queue");
            managed.record.state = RunState::Planning;
            let event = next_event(
                managed,
                inner.ports.clock.now(),
                RunEventKind::StateChanged {
                    state: RunState::Planning,
                },
            );
            if let Err(error) = inner.ports.history.update(&managed.record) {
                state.background_error = Some(error.to_string());
                state.active_slots = state.active_slots.saturating_sub(1);
                inner.wake.notify_all();
                return;
            }
            (run_id, event)
        };
        inner.publish_event(dispatched.1);
        let worker_inner = Arc::clone(&inner);
        if let Err(error) = thread::Builder::new()
            .name(format!("foldry-run-{}", dispatched.0))
            .spawn(move || execute_run(worker_inner, dispatched.0))
        {
            inner.store_background_error(error.to_string());
            return;
        }
    }
}

fn execute_run(inner: Arc<SchedulerInner>, run_id: RunId) {
    let prepared = {
        let mut state = match inner.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        let managed = match state.runs.get_mut(&run_id) {
            Some(managed) => managed,
            None => return,
        };
        let prepared = match managed.record.state {
            RunState::Planning => {
                managed.record.state = RunState::Running;
                let event = next_event(
                    managed,
                    inner.ports.clock.now(),
                    RunEventKind::StateChanged {
                        state: RunState::Running,
                    },
                );
                Some((managed.record.clone(), managed.control.clone(), Some(event)))
            }
            RunState::Paused => Some((managed.record.clone(), managed.control.clone(), None)),
            RunState::Stopping => None,
            _ => None,
        };
        if let Some((record, _, _)) = &prepared {
            if let Err(error) = inner.ports.history.update(record) {
                state.background_error = Some(error.to_string());
                None
            } else {
                prepared
            }
        } else {
            prepared
        }
    };
    let Some((record, control, state_event)) = prepared else {
        finish_run(&inner, run_id, stopped_summary());
        return;
    };
    if let Some(state_event) = state_event {
        inner.publish_event(state_event);
    }
    let reporter = SchedulerReporter {
        inner: Arc::clone(&inner),
        run_id,
    };
    let summary = inner.ports.executor.execute(&record, &control, &reporter);
    finish_run(&inner, run_id, summary);
}

fn finish_run(inner: &Arc<SchedulerInner>, run_id: RunId, mut summary: ResultSummary) {
    let completed = {
        let mut state = match inner.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        let managed = match state.runs.get_mut(&run_id) {
            Some(managed) => managed,
            None => return,
        };
        if managed.record.state == RunState::Stopping || managed.control.is_stopped() {
            summary = stopped_summary();
        }
        let terminal = outcome_state(summary.outcome);
        if let Err(error) = validate_transition(managed.record.state, terminal) {
            state.background_error = Some(error.to_string());
            state.active_slots = state.active_slots.saturating_sub(1);
            inner.wake.notify_all();
            return;
        }
        managed.record.state = terminal;
        managed.record.finished_at = Some(inner.ports.clock.now());
        managed.record.summary = Some(summary.clone());
        let state_event = next_event(
            managed,
            inner.ports.clock.now(),
            RunEventKind::StateChanged { state: terminal },
        );
        let completed_event = next_event(
            managed,
            inner.ports.clock.now(),
            RunEventKind::Completed { summary },
        );
        if let Err(error) = inner.ports.history.update(&managed.record) {
            state.background_error = Some(error.to_string());
        }
        state.active_slots = state.active_slots.saturating_sub(1);
        inner.wake.notify_all();
        (state_event, completed_event)
    };
    inner.publish_event(completed.0);
    inner.publish_event(completed.1);
}

struct SchedulerReporter {
    inner: Arc<SchedulerInner>,
    run_id: RunId,
}

impl RunReporter for SchedulerReporter {
    fn progress(&self, progress: ProgressSnapshot) {
        let event = self.inner.state.lock().ok().and_then(|mut state| {
            let run = state.runs.get_mut(&self.run_id)?;
            let now = Instant::now();
            if run
                .last_progress_event
                .is_some_and(|previous| now.duration_since(previous) < self.inner.progress_interval)
            {
                return None;
            }
            run.last_progress_event = Some(now);
            Some(next_event(
                run,
                self.inner.ports.clock.now(),
                RunEventKind::Progress { progress },
            ))
        });
        if let Some(event) = event {
            self.inner.publish_event(event);
        }
    }

    fn warning(&self, warning: FoldryWarning) {
        self.inner
            .report_event(self.run_id, RunEventKind::Warning { warning });
    }

    fn error(&self, error: FoldryError) {
        self.inner
            .report_event(self.run_id, RunEventKind::Error { error });
    }

    fn log(&self, level: LogLevel, message: String, path: Option<String>) {
        let record = self.inner.state.lock().ok().and_then(|mut state| {
            let run = state.runs.get_mut(&self.run_id)?;
            let sequence = run.log_sequence;
            run.log_sequence = run.log_sequence.saturating_add(1);
            Some(LogRecord {
                run_id: self.run_id,
                sequence,
                occurred_at: self.inner.ports.clock.now(),
                level,
                message,
                path,
            })
        });
        if let Some(record) = record
            && let Err(error) = self.inner.ports.logs.append(&record)
        {
            self.inner.store_background_error(error.to_string());
        }
    }
}

#[must_use]
pub const fn is_terminal(state: RunState) -> bool {
    matches!(
        state,
        RunState::Succeeded
            | RunState::SucceededWithWarnings
            | RunState::Failed
            | RunState::Stopped
            | RunState::Interrupted
    )
}

pub fn validate_transition(from: RunState, to: RunState) -> Result<(), SchedulerError> {
    let valid = matches!(
        (from, to),
        (RunState::Queued, RunState::Planning | RunState::Stopped)
            | (
                RunState::Planning,
                RunState::Running
                    | RunState::Paused
                    | RunState::Stopping
                    | RunState::Failed
                    | RunState::Stopped
            )
            | (
                RunState::Running,
                RunState::Paused
                    | RunState::Stopping
                    | RunState::Succeeded
                    | RunState::SucceededWithWarnings
                    | RunState::Failed
                    | RunState::Stopped
                    | RunState::Interrupted
            )
            | (
                RunState::Paused,
                RunState::Running
                    | RunState::Stopping
                    | RunState::Succeeded
                    | RunState::SucceededWithWarnings
                    | RunState::Failed
                    | RunState::Stopped
                    | RunState::Interrupted
            )
            | (
                RunState::Stopping,
                RunState::Stopped | RunState::Failed | RunState::Interrupted
            )
    );
    if valid {
        Ok(())
    } else {
        Err(SchedulerError::InvalidTransition { from, to })
    }
}

fn next_event(run: &mut ManagedRun, occurred_at: jiff::Timestamp, event: RunEventKind) -> RunEvent {
    let sequence = run.event_sequence;
    run.event_sequence = run.event_sequence.saturating_add(1);
    RunEvent {
        version: 1,
        run_id: run.record.run_id,
        task_id: run.record.task_id,
        sequence,
        occurred_at,
        event,
        extensions: Extensions::new(),
    }
}

fn outcome_state(outcome: RunOutcome) -> RunState {
    match outcome {
        RunOutcome::Succeeded => RunState::Succeeded,
        RunOutcome::SucceededWithWarnings => RunState::SucceededWithWarnings,
        RunOutcome::Failed => RunState::Failed,
        RunOutcome::Stopped => RunState::Stopped,
        RunOutcome::Interrupted => RunState::Interrupted,
    }
}

fn stopped_summary() -> ResultSummary {
    ResultSummary {
        outcome: RunOutcome::Stopped,
        included_entries: 0,
        skipped_entries: 0,
        source_bytes: 0,
        duration_ms: 0,
        artifact: None,
        warnings: Vec::new(),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_accepts_only_documented_transitions() {
        assert!(validate_transition(RunState::Queued, RunState::Planning).is_ok());
        assert!(validate_transition(RunState::Running, RunState::Paused).is_ok());
        assert!(validate_transition(RunState::Paused, RunState::Running).is_ok());
        assert!(validate_transition(RunState::Stopping, RunState::Stopped).is_ok());
        assert!(validate_transition(RunState::Queued, RunState::Succeeded).is_err());
        assert!(validate_transition(RunState::Succeeded, RunState::Running).is_err());
    }

    #[test]
    fn terminal_state_classification_is_explicit() {
        assert!(is_terminal(RunState::Failed));
        assert!(is_terminal(RunState::Interrupted));
        assert!(!is_terminal(RunState::Paused));
        assert!(!is_terminal(RunState::Stopping));
    }
}

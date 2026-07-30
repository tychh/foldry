use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use foldry_application::{
    Clock, Extensions, FolderId, FolderSnapshot, LogLevel, LogRepository, PageRequest,
    ProgressPhase, ProgressSnapshot, ResultSummary, RunEvent, RunEventKind, RunEventSink,
    RunExecutor, RunHistoryRepository, RunId, RunOutcome, RunRecord, RunReporter, RunSnapshot,
    RunState, Scheduler, SchedulerPorts, Settings,
};
use foldry_storage::{SqliteRepository, decode_plan};
use jiff::Timestamp;

#[derive(Clone, Copy)]
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        "2026-07-27T12:00:00Z".parse().unwrap()
    }
}

#[derive(Default)]
struct CollectingEvents {
    events: Mutex<Vec<RunEvent>>,
    changed: Condvar,
}

impl RunEventSink for CollectingEvents {
    fn publish(&self, event: RunEvent) {
        self.events.lock().unwrap().push(event);
        self.changed.notify_all();
    }
}

impl CollectingEvents {
    fn wait_for_terminal(&self, run_id: RunId) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut events = self.events.lock().unwrap();
        while !events.iter().any(|event| {
            event.run_id == run_id && matches!(event.event, RunEventKind::Completed { .. })
        }) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "run {run_id} did not complete");
            events = self.changed.wait_timeout(events, remaining).unwrap().0;
        }
    }

    fn wait_for_completed_count(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = self.events.lock().unwrap();
        while events
            .iter()
            .filter(|event| matches!(event.event, RunEventKind::Completed { .. }))
            .count()
            < count
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "only part of the queue completed");
            events = self.changed.wait_timeout(events, remaining).unwrap().0;
        }
    }
}

#[derive(Default)]
struct ControlledExecutor {
    state: Mutex<ControlState>,
    changed: Condvar,
}

#[derive(Default)]
struct ControlState {
    started: Vec<FolderId>,
    released: HashSet<FolderId>,
    active: usize,
    max_active: usize,
}

impl ControlledExecutor {
    fn wait_for_started(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state.lock().unwrap();
        while state.started.len() < count {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "only {:?} started", state.started);
            state = self.changed.wait_timeout(state, remaining).unwrap().0;
        }
    }

    fn release(&self, folder_id: FolderId) {
        self.state.lock().unwrap().released.insert(folder_id);
        self.changed.notify_all();
    }

    fn started(&self) -> Vec<FolderId> {
        self.state.lock().unwrap().started.clone()
    }
}

impl RunExecutor for ControlledExecutor {
    fn execute(
        &self,
        run: &RunRecord,
        control: &foldry_application::ExecutionControl,
        reporter: &dyn RunReporter,
    ) -> ResultSummary {
        let folder_id = run.snapshot.folder.id;
        {
            let mut state = self.state.lock().unwrap();
            state.started.push(folder_id);
            state.active += 1;
            state.max_active = state.max_active.max(state.active);
            self.changed.notify_all();
        }
        let stopped = loop {
            if !control.checkpoint() {
                break true;
            }
            reporter.progress(progress(1));
            let mut state = self.state.lock().unwrap();
            if state.released.remove(&folder_id) {
                break false;
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(10))
                .unwrap()
                .0;
            drop(state);
        };
        let mut state = self.state.lock().unwrap();
        state.active -= 1;
        self.changed.notify_all();
        if stopped {
            summary(RunOutcome::Stopped)
        } else {
            summary(RunOutcome::Succeeded)
        }
    }
}

struct BurstExecutor;

impl RunExecutor for BurstExecutor {
    fn execute(
        &self,
        _run: &RunRecord,
        _control: &foldry_application::ExecutionControl,
        reporter: &dyn RunReporter,
    ) -> ResultSummary {
        for index in 0..100 {
            reporter.progress(progress(index));
        }
        reporter.warning(foldry_application::FoldryWarning {
            code: foldry_application::WarningCode::SpecialFileSkipped,
            message: "warning".into(),
            path: None,
            extensions: Extensions::new(),
        });
        reporter.log(LogLevel::Info, "detail".into(), None);
        summary(RunOutcome::SucceededWithWarnings)
    }
}

#[derive(Default)]
struct StressExecutor {
    active: AtomicUsize,
    max_active: AtomicUsize,
}

impl RunExecutor for StressExecutor {
    fn execute(
        &self,
        _run: &RunRecord,
        _control: &foldry_application::ExecutionControl,
        _reporter: &dyn RunReporter,
    ) -> ResultSummary {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(2));
        self.active.fetch_sub(1, Ordering::SeqCst);
        summary(RunOutcome::Succeeded)
    }
}

#[test]
fn fifo_queue_respects_parallel_limit_and_starts_next_after_completion() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let executor = Arc::new(ControlledExecutor::default());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = scheduler(Arc::clone(&repository), executor.clone(), events.clone(), 2);
    let runs = (0..4).map(|_| sample_run()).collect::<Vec<_>>();
    for run in &runs {
        scheduler.enqueue(run.clone()).unwrap();
    }

    executor.wait_for_started(2);
    assert_eq!(executor.state.lock().unwrap().max_active, 2);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(executor.started().len(), 2);

    executor.release(runs[0].folder_id);
    events.wait_for_terminal(runs[0].run_id);
    executor.wait_for_started(3);
    executor.release(runs[1].folder_id);
    executor.release(runs[2].folder_id);
    executor.wait_for_started(4);
    executor.release(runs[3].folder_id);
    for run in &runs {
        events.wait_for_terminal(run.run_id);
    }

    let planning_order = events
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| {
            matches!(
                event.event,
                RunEventKind::StateChanged {
                    state: RunState::Planning
                }
            )
            .then_some(event.run_id)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        planning_order,
        runs.iter().map(|run| run.run_id).collect::<Vec<_>>()
    );
}

#[test]
fn paused_run_keeps_its_slot_and_stop_wakes_it_for_the_next_run() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let executor = Arc::new(ControlledExecutor::default());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = scheduler(Arc::clone(&repository), executor.clone(), events.clone(), 1);
    let first = sample_run();
    let second = sample_run();
    scheduler.enqueue(first.clone()).unwrap();
    scheduler.enqueue(second.clone()).unwrap();
    executor.wait_for_started(1);

    assert!(scheduler.pause(first.run_id).unwrap());
    assert!(!scheduler.pause(first.run_id).unwrap());
    assert_eq!(
        scheduler.record(first.run_id).unwrap().state,
        RunState::Paused
    );
    thread::sleep(Duration::from_millis(150));
    assert_eq!(executor.started().len(), 1);

    assert!(scheduler.stop(first.run_id).unwrap());
    assert!(!scheduler.stop(first.run_id).unwrap());
    events.wait_for_terminal(first.run_id);
    executor.wait_for_started(2);
    executor.release(second.folder_id);
    events.wait_for_terminal(second.run_id);
    assert_eq!(
        scheduler.record(first.run_id).unwrap().state,
        RunState::Stopped
    );
}

#[test]
fn global_pause_holds_queued_work_until_resume() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let executor = Arc::new(ControlledExecutor::default());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = scheduler(Arc::clone(&repository), executor.clone(), events.clone(), 1);
    let run = sample_run();

    assert_eq!(scheduler.pause_all().unwrap(), 0);
    scheduler.enqueue(run.clone()).unwrap();
    thread::sleep(Duration::from_millis(150));
    assert!(executor.started().is_empty());
    assert_eq!(scheduler.resume_all().unwrap(), 0);
    executor.wait_for_started(1);
    executor.release(run.folder_id);
    events.wait_for_terminal(run.run_id);
}

#[test]
fn global_stop_clears_pause_and_allows_a_followup_run() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let executor = Arc::new(ControlledExecutor::default());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = scheduler(Arc::clone(&repository), executor.clone(), events.clone(), 1);
    let stopped = sample_run();
    let followup = sample_run();

    assert_eq!(scheduler.pause_all().unwrap(), 0);
    scheduler.enqueue(stopped.clone()).unwrap();
    thread::sleep(Duration::from_millis(150));
    assert!(executor.started().is_empty());

    assert_eq!(scheduler.stop_all().unwrap(), 1);
    events.wait_for_terminal(stopped.run_id);
    assert_eq!(
        scheduler.record(stopped.run_id).unwrap().state,
        RunState::Stopped
    );

    scheduler.enqueue(followup.clone()).unwrap();
    executor.wait_for_started(1);
    executor.release(followup.folder_id);
    events.wait_for_terminal(followup.run_id);
    assert_eq!(
        scheduler.record(followup.run_id).unwrap().state,
        RunState::Succeeded
    );
}

#[test]
fn global_stop_finalizes_active_and_queued_runs() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let executor = Arc::new(ControlledExecutor::default());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = scheduler(Arc::clone(&repository), executor.clone(), events.clone(), 1);
    let runs = (0..3).map(|_| sample_run()).collect::<Vec<_>>();
    for run in &runs {
        scheduler.enqueue(run.clone()).unwrap();
    }
    executor.wait_for_started(1);

    assert_eq!(scheduler.stop_all().unwrap(), 3);
    for run in &runs {
        events.wait_for_terminal(run.run_id);
        assert_eq!(
            scheduler.record(run.run_id).unwrap().state,
            RunState::Stopped
        );
    }
    assert_eq!(executor.started().len(), 1);
}

#[test]
fn progress_is_throttled_while_state_warning_final_and_logs_are_immediate() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = scheduler(
        Arc::clone(&repository),
        Arc::new(BurstExecutor),
        events.clone(),
        1,
    );
    let run = sample_run();
    scheduler.enqueue(run.clone()).unwrap();
    events.wait_for_terminal(run.run_id);

    let collected = events.events.lock().unwrap();
    assert_eq!(
        collected
            .iter()
            .filter(|event| matches!(event.event, RunEventKind::Progress { .. }))
            .count(),
        1
    );
    assert!(
        collected
            .iter()
            .any(|event| matches!(event.event, RunEventKind::Warning { .. }))
    );
    assert!(matches!(
        collected.last().unwrap().event,
        RunEventKind::Completed { .. }
    ));
    drop(collected);
    assert_eq!(
        LogRepository::page(
            repository.as_ref(),
            run.run_id,
            PageRequest {
                offset: 0,
                limit: 10
            }
        )
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn concurrent_commands_are_idempotent_and_persistence_matches_the_final_state() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let executor = Arc::new(ControlledExecutor::default());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = Arc::new(scheduler(
        Arc::clone(&repository),
        executor.clone(),
        events.clone(),
        1,
    ));
    let run = sample_run();
    scheduler.enqueue(run.clone()).unwrap();
    executor.wait_for_started(1);

    let commands = (0..8)
        .map(|_| {
            let scheduler = Arc::clone(&scheduler);
            thread::spawn(move || {
                for _ in 0..50 {
                    let _ = scheduler.pause(run.run_id);
                    let _ = scheduler.resume(run.run_id);
                }
            })
        })
        .collect::<Vec<_>>();
    for command in commands {
        command.join().unwrap();
    }
    assert!(scheduler.stop(run.run_id).unwrap());
    events.wait_for_terminal(run.run_id);

    assert_eq!(
        scheduler.record(run.run_id).unwrap().state,
        RunState::Stopped
    );
    assert_eq!(
        repository.get(run.run_id).unwrap().unwrap().state,
        RunState::Stopped
    );
    let sequences = events
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.run_id == run.run_id)
        .map(|event| event.sequence)
        .collect::<Vec<_>>();
    assert!(
        sequences
            .windows(2)
            .all(|pair| pair[1] == pair[0].saturating_add(1))
    );
}

#[test]
fn stress_queue_never_exceeds_its_limit_and_preserves_dispatch_order() {
    let repository = Arc::new(SqliteRepository::open_in_memory().unwrap());
    let executor = Arc::new(StressExecutor::default());
    let events = Arc::new(CollectingEvents::default());
    let scheduler = scheduler(Arc::clone(&repository), executor.clone(), events.clone(), 4);
    let runs = (0..100).map(|_| sample_run()).collect::<Vec<_>>();
    for run in &runs {
        scheduler.enqueue(run.clone()).unwrap();
    }

    events.wait_for_completed_count(runs.len());

    assert!(executor.max_active.load(Ordering::SeqCst) <= 4);
    let planning_order = events
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| {
            matches!(
                event.event,
                RunEventKind::StateChanged {
                    state: RunState::Planning
                }
            )
            .then_some(event.run_id)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        planning_order,
        runs.iter().map(|run| run.run_id).collect::<Vec<_>>()
    );
}

fn scheduler(
    repository: Arc<SqliteRepository>,
    executor: Arc<dyn RunExecutor>,
    events: Arc<CollectingEvents>,
    limit: u16,
) -> Scheduler {
    let history: Arc<dyn RunHistoryRepository> = repository.clone();
    let logs: Arc<dyn LogRepository> = repository;
    Scheduler::start(
        SchedulerPorts {
            history,
            logs,
            clock: Arc::new(FixedClock),
            executor,
            events,
        },
        limit,
    )
    .unwrap()
}

fn sample_run() -> RunRecord {
    let mut folder = fixture_task();
    folder.id = FolderId::new();
    let action = folder.actions.remove(0);
    RunRecord {
        run_id: RunId::new(),
        folder_id: folder.id,
        action_id: action.id,
        state: RunState::Queued,
        started_at: "2026-07-27T12:00:00Z".parse().unwrap(),
        finished_at: None,
        snapshot: RunSnapshot {
            folder: FolderSnapshot {
                id: folder.id,
                source: folder.source,
            },
            action,
            effective_profile_id: folder.default_profile_id,
            settings: Settings::default(),
            profile_text: "# snapshot".into(),
            profile_hash: "hash".into(),
        },
        summary: None,
    }
}

fn fixture_task() -> foldry_application::Folder {
    let source = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/formats/v2/plan.packplan.yaml"),
    )
    .unwrap();
    decode_plan(&source).unwrap().folders.remove(0)
}

fn progress(value: u64) -> ProgressSnapshot {
    ProgressSnapshot {
        phase: ProgressPhase::Archiving,
        completed_entries: value,
        total_entries: Some(100),
        completed_bytes: value,
        total_bytes: Some(100),
        current_path: None,
    }
}

fn summary(outcome: RunOutcome) -> ResultSummary {
    ResultSummary {
        outcome,
        included_entries: 1,
        skipped_entries: 0,
        source_bytes: 1,
        duration_ms: 1,
        artifact: None,
        warnings: Vec::new(),
        error: None,
    }
}

# Scheduler contract v1

## Queue and slots

The scheduler owns a FIFO queue and accepts a configured parallel limit from 1 to 64. Dispatch order is stable. A run occupies a slot from `planning` until its
terminal state.

A paused run keeps its slot. It blocks at the executor's entry checkpoint, consumes
no archive work, and prevents another queued run from taking that slot. Stopping a
paused run wakes the checkpoint, finalizes it as `stopped`, releases the slot, and
allows the next queued run to start.

Global pause prevents new queue dispatch and pauses active planning/running work.
Global resume releases both constraints. Global stop applies the same idempotent
per-run stop operation to every non-terminal run.

## State machine

The v1 runtime uses these principal transitions:

```text
queued -> planning -> running <-> paused
queued --------------------------> stopped
planning/running/paused -> stopping -> stopped
running/paused -> succeeded | succeeded_with_warnings | failed | interrupted
```

Invalid transitions return a typed scheduler error. Repeated pause, resume, or stop
commands that are already satisfied are successful no-ops. Command changes and
their history updates are serialized so concurrent commands cannot leave SQLite
behind the in-memory state.

## Execution boundary

`RunExecutor` receives an immutable `RunSnapshot`, a shared `ExecutionControl`, and
a `RunReporter`. This keeps scheduling independent from archive planning and makes
the same scheduler usable by CLI and desktop adapters.

Current-state runs snapshot the selected task, application settings, exact profile
text, and profile hash. `Run all` prepares only enabled tasks. Historical repeat
uses the former snapshot unchanged.

## Events and logs

State changes, warnings, errors, and the final summary are queued immediately.
Per-run event sequence numbers are delivered in strict order even if worker and
command threads race.

Progress uses a monotonic 100 ms interval, so a run emits at most 10 progress updates
per second. Dropped intermediate progress does not affect persisted final totals.

Detailed log records use a separate per-run sequence and are written directly to
the log repository. They are not copied into the UI event stream and remain paged
on read.

## Shutdown and recovery

Dropping the scheduler stops all non-terminal controls and terminates dispatcher
threads. If the process exits before workers persist their terminal state, startup
reconciliation changes the remaining unfinished database rows to `interrupted` and
applies the ownership checks from the persistence contract.

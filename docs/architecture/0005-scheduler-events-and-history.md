# ADR-0005: Scheduler, события, logs и история

- Статус: Accepted
- Дата: 2026-07-26

## Контекст

Пользователь может запустить одну или все задачи, ограничить параллельность,
приостановить/остановить отдельные или все runs и после перезапуска посмотреть
результат. Частые progress/log events не должны перегружать Tauri/webview.

## Решение

### Task, run и очередь

- Task — сохраняемая конфигурация source/profile/action steps.
- Run — неизменяемый snapshot эффективной конфигурации конкретного запуска.
- Одна task имеет много runs.
- Scheduler использует FIFO для готовых задач и configurable
  `max_concurrent_tasks`.
- Один canonical source path встречается в активном plan один раз.
- Output path резервируется executor до записи, поэтому CLI и GUI могут безопасно
  работать одновременно.

### State machine

Наблюдаемые состояния:

```text
queued -> planning -> running -> completed
   |          |          |
   |          |          +-> pause_requested -> paused -> running
   |          |          +-> cancelling -> cancelled
   |          +------------> failed/cancelled
   +-----------------------> cancelled

planning/running/paused -> interrupted  (process crash/restart)
```

- Недопустимый переход отклоняется typed application error.
- Повторные pause/resume/stop идемпотентны.
- Global pause закрывает start gate для queued runs и отправляет pause активным.
- Paused run сохраняет concurrency slot, не потребляет CPU и не начинает новый entry.
- Stopped/cancelled run освобождает slot, после чего стартует следующий queued run.
- Global stop отменяет queued и кооперативно останавливает активные runs.

### События

Каналы разделены:

- Progress агрегируется не чаще 10 обновлений в секунду на run.
- State transition, command acknowledgement, warning, error и final summary
  отправляются немедленно и не проходят progress throttle.
- Detailed logs пишутся в storage и не транслируются целиком.
- GUI запрашивает logs страницами только при открытии.
- Каждая команда и событие содержит task ID, run ID и correlation ID, где применимо.
- После reload/reconnect GUI запрашивает полный current snapshot; backend run не
  зависит от жизненного цикла webview.

### Результат и история

Run summary содержит:

- outcome и timestamps;
- output path, format, size и optional SHA-256;
- processed/included/skipped file counts и bytes;
- warnings;
- structured error;
- ссылку на detailed logs;
- snapshot settings, action steps и profile text/hash.

`Повторить` создает новый run из snapshot. Отдельное действие запускает task с
текущими настройками. Если старый output недоступен, требуется новый output, старый
run не изменяется.

Retention и privacy определены в ADR-0003.

## Последствия

- Soft pause намеренно уменьшает фактический throughput: paused run продолжает
  занимать slot.
- Snapshot увеличивает размер history, но обеспечивает воспроизводимость.
- UI обязан различать success, success-with-warnings, failed, cancelled и
  interrupted.
- Logs pagination и current snapshot нужны и CLI, и Tauri adapters.

## Проверка

- Deterministic scheduler tests используют fake clock/executor.
- Stress tests перебирают гонки pause/resume/stop и изменение concurrency.
- За 10 секунд steady progress каждый run отправляет не более 100 progress events.
- State/error/final events не задерживаются progress throttle.
- Reload webview не отменяет run и восстанавливает его состояние.
- Restart reconciliation переводит незавершенные runs в `interrupted`.

## История

- 2026-07-26 — решение принято для начала реализации.

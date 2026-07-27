# ADR-0003: Хранение данных и lifecycle

- Статус: Accepted
- Дата: 2026-07-26

## Контекст

Установочная папка desktop-приложения обычно недоступна для записи. Profiles и plan
должны быть доступны пользователю как текст, runtime history требует транзакций, а
временные manifests не должны смешиваться с долговечными данными. Обновление
приложения не должно уничтожать ручные изменения.

## Решение

### Каталоги

Конкретные native paths определяются системным API по identifier
`app.foldry.desktop`, а не составляются вручную.

```text
platform config/
├── settings.yaml
├── active.packplan.yaml
├── profiles/
└── presets/

platform data/
├── app.db
└── crash-reports/

platform cache/
└── manifests/
```

- Официального portable mode нет.
- CLI path overrides разрешены для development, tests и recovery, но не являются
  публичным portable-контрактом.
- GUI показывает фактические каталоги и умеет открыть config/data directory.

### Поставляемые данные

- Эталонный preset catalog поставляется в read-only application resources.
- При первом запуске отсутствующие рабочие profile/preset файлы создаются в config.
- Существующий рабочий файл никогда не заменяется молча при upgrade.
- Более новая ресурсная версия делает рабочую копию/preset-блок `outdated`; обновление
  или reset выполняются явным действием.
- Удаленный Default можно воссоздать из поставляемых presets.

### Источник истины

- `active.packplan.yaml` хранит настроенные задачи и overrides.
- `settings.yaml` хранит defaults и application settings.
- SQLite хранит runs, snapshots, summaries, warnings/errors, logs и ссылку на
  активный plan, но не копию текущих задач.
- Run snapshot содержит эффективные settings и текст/hash profile, необходимые для
  воспроизводимого `Повторить`.

### Атомарная запись

Для settings, plan и profiles:

1. Создать уникальный временный файл в том же каталоге.
2. Записать и закрыть содержимое; выполнить durability flush согласно платформенной
   реализации.
3. Валидировать записанный формат.
4. Атомарно заменить целевой файл.
5. Не удалять предыдущую рабочую версию до успешной публикации.

Profile repository дополнительно хранит одну previous-good копию при сохранении
невалидного текста. Временные файлы имеют owner/run ID и cleanup не удаляет
неизвестные файлы.

### SQLite и migrations

- Схема изменяется только последовательными migration scripts.
- Migration выполняется транзакционно до запуска scheduler.
- Более новая неподдерживаемая схема не понижается и не перезаписывается.
- Оборванные активные runs при старте становятся `interrupted`.
- Reconciliation удаляет только подтвержденные stale artifacts, принадлежащие
  Foldry и не принадлежащие активному процессу.

### Retention

- Run metadata: не более одного года и не более 10 000 последних runs.
- Detailed logs: не более 90 дней и не более 1 000 последних runs.
- В каждой категории действует более строгая граница по возрасту/количеству.
- Настройка `unlimited` отключает автоматическое удаление соответствующей категории.
- Очистка history/logs/crash reports не удаляет созданные архивы.

### Приватность

- Телеметрии нет.
- Crash reports создаются только локально и не отправляются по сети.
- Paths не покидают устройство автоматически.
- Пользователь может отдельно очистить history, logs, crash reports и recent paths.

## Последствия

- Пользовательские файлы переживают upgrade/reinstall согласно правилам платформы.
- Для тестов нужен изолированный набор config/data/cache overrides.
- Reconciliation и ownership metadata обязательны для безопасного cleanup.
- БД может расти ограниченно и предсказуемо.

## Проверка

- Fault-injection tests обрывают процесс на каждом шаге атомарной записи.
- Migration tests открывают fixtures каждой выпущенной schema version.
- Upgrade fixture подтверждает, что измененный Default/preset не перезаписывается.
- Retention tests используют управляемые clock и количество runs.
- Network audit не обнаруживает telemetry/crash upload endpoints.

## История

- 2026-07-26 — решение принято для начала реализации.

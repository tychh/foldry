# Acceptance checklist Foldry

Этот документ является проверяемым определением готовности v1 и v1.1. Пункты
отмечаются выполненными только вместе со ссылкой на автоматический тест, CI run,
скриншот или запись ручной проверки.

## Правила приемки

- `[ ]` — не проверено; `[x]` — проверено с приложенным evidence.
- Все пункты без пометки v1.1 обязательны для v1.
- Пункты `MANUAL` проверяются на реальной ОС; остальные по возможности
  автоматизируются.
- Один тест может закрывать несколько пунктов, но evidence указывается у каждого.
- Release candidate не выпускается при открытом решении в `notes/decisions.md`,
  failing CI, повреждении старого архива или тихом пропуске данных.
- Поддерживаемые версии ОС окончательно фиксируются перед packaging на основе
  требований актуальной стабильной Tauri. CI и документация используют один список.

## A. Сборка и архитектурные границы

- [ ] `AC-ARCH-001` Workspace содержит `foldry-core`, `foldry-application`,
      `foldry-storage`, `foldry-cli` и `foldry-tauri`; frontend находится отдельно.
- [ ] `AC-ARCH-002` `foldry-core` не зависит от Tauri, React/Node или SQLite.
- [ ] `AC-ARCH-003` CLI и Tauri вызывают одни application use cases, а не имеют
      отдельную реализацию filtering/archive/scheduler.
- [ ] `AC-ARCH-004` TypeScript IPC DTO генерируются из Rust-контрактов; повторная
      генерация в CI не меняет working tree.
- [ ] `AC-ARCH-005` Неизвестный action `type` не выполняется, сохраняется там, где
      round-trip безопасен, и показывается как unsupported.
- [ ] `AC-ARCH-006` Linux, Windows и macOS CI выполняют format, lint, typecheck,
      unit/integration tests и build.
- [ ] `AC-ARCH-007` Product metadata использует имя Foldry, identifier
      `app.foldry.desktop`, packages `foldry-*` и CLI `foldry`; alias `ab` отсутствует.

## B. Форматы, версии и migrations

- [ ] `AC-FMT-001` Valid settings/plan/profile fixtures читаются и проходят
      round-trip без потери значимых данных.
- [ ] `AC-FMT-002` Поврежденный YAML дает structured diagnostic с путем к полю и не
      перезаписывается автоматически.
- [ ] `AC-FMT-003` Неподдерживаемая future major version блокируется понятной
      ошибкой, не вызывая panic или downgrade.
- [ ] `AC-FMT-004` Fixtures каждой выпущенной версии читаются либо мигрируют в CI.
- [ ] `AC-FMT-005` Migration SQLite выполняется транзакционно; имитация ошибки
      оставляет предыдущую схему рабочей.
- [ ] `AC-FMT-006` `.packplan.yaml` использует versioned tasks/steps и один task на
      canonical source path.
- [ ] `AC-FMT-007` В v1 task принимает ровно один поддерживаемый step `archive`, но
      parser не предполагает, что список всегда будет длины один.
- [ ] `AC-FMT-008` Compression сохраняется как `fast|balanced|maximum`, а mapping
      codec version 1 совпадает с ADR-0004.

## C. Profiles и matcher

- [ ] `AC-PRO-001` Profile — UTF-8 `.packignore` с UUIDv7 `@profile-id` и
      `@profile-name`.
- [ ] `AC-PRO-002` Rename profile меняет display name/filename, но сохраняет ID.
- [ ] `AC-PRO-003` Duplicate profile ID обнаруживается и не выбирается молча.
- [ ] `AC-PRO-004` Parser поддерживает comments, escaping, negation, anchors,
      directory-only, `*`, `?`, `[]` и `**`.
- [ ] `AC-PRO-005` Правила применяются сверху вниз; последнее совпадение определяет
      include/exclude.
- [ ] `AC-PRO-006` Re-include следует Git-compatible pruning и требует возврата
      родительского каталога.
- [ ] `AC-PRO-007` Matching использует normalized `/`, а case-sensitivity
      соответствует source filesystem.
- [ ] `AC-PRO-008` Для include/exclude доступна причина: profile ID, строка, текст
      правила и preset ID.
- [ ] `AC-PRO-009` Default исключает runtime/build/cache/VCS artifacts и не исключает
      secrets, local config, dumps или private media.
- [ ] `AC-PRO-010` Default редактируется и удаляется как обычный файл; restore
      создает его из поставляемых presets.
- [ ] `AC-PRO-011` Невалидный profile атомарно сохраняется с previous-good копией,
      показывает diagnostics и не запускается.
- [ ] `AC-PRO-012` Полная пользовательская спецификация `.packignore` содержит все
      отличия от `.gitignore` и примеры re-include.

## D. Presets

- [ ] `AC-PRS-001` Поставлены presets из списка task.md: языки, frameworks, IDE, ОС,
      test/coverage/build artifacts.
- [ ] `AC-PRS-002` Sensitive presets отделены, маркированы и требуют warning перед
      вставкой.
- [ ] `AC-PRS-003` Marker-блок содержит ID/version; второй блок с тем же ID не
      вставляется.
- [ ] `AC-PRS-004` Состояния `absent`, `installed`, `modified`, `outdated`
      соответствуют алгоритму ADR-0002.
- [ ] `AC-PRS-005` Повторный клик удаляет неизмененный preset; modified preset
      требует inline confirmation внутри карточки.
- [ ] `AC-PRS-006` Ручное изменение блока не позволяет вставить preset второй раз.
- [ ] `AC-PRS-007` Upgrade приложения не перезаписывает измененную рабочую копию;
      reset явно восстанавливает ресурсную версию.
- [ ] `AC-PRS-008` v1.1: пользователь может создать, переименовать, изменить и удалить
      собственный preset со стабильным ID.

## E. Filesystem browser и drag-and-drop

- [ ] `AC-FS-001` Browser показывает Home, filesystem roots/drives и favorites;
      полный filesystem доступен через root/drive.
- [ ] `AC-FS-002` Раскрытие узла запрашивает только direct children; закрытые ветки
      не сканируются заранее.
- [ ] `AC-FS-003` Устаревший запрос children отменяется/игнорируется после
      сворачивания или смены каталога.
- [ ] `AC-FS-004` Unreadable nodes видимы как недоступные и не вызывают зависание.
- [ ] `AC-FS-005` Symlink/junction и mount/network nodes имеют признаки, не
      основанные только на цвете.
- [ ] `AC-FS-006` Drop одной папки создает одну задачу; несколько папок — несколько
      задач.
- [ ] `AC-FS-007` Dropped file игнорируется с объяснением.
- [ ] `AC-FS-008` Duplicate canonical source не создает задачу и фокусирует
      существующую карточку.
- [ ] `AC-FS-009` Browser и drag-and-drop корректно работают с Unicode paths.

## F. Scanner и preview

- [ ] `AC-SCAN-001` Обычные файлы включаются, directories обходятся, symlink и
      junction никогда не обходятся.
- [ ] `AC-SCAN-002` Mount/network directory обходится как directory, если доступен и
      входит в source.
- [ ] `AC-SCAN-003` Special file пропускается с warning и отражается в summary.
- [ ] `AC-SCAN-004` Default unreadable/missing/changed entry завершает run ошибкой.
- [ ] `AC-SCAN-005` `warn_and_skip` продолжает run, добавляет warning и дает outcome
      success-with-warnings.
- [ ] `AC-SCAN-006` Preview выдается страницами/виртуализируется и показывает
      include/exclude с причиной.
- [ ] `AC-SCAN-007` Preview показывает время и profile hash.
- [ ] `AC-SCAN-008` Изменение profile/source/action инвалидирует preview cache.
- [ ] `AC-SCAN-009` Run всегда строит новый execution plan и не использует preview
      как authoritative snapshot.
- [ ] `AC-SCAN-010` Planner пишет entries в streaming manifest и успешно обрабатывает
      synthetic tree из 1 000 000 entries под установленным для CI memory limit.
- [ ] `AC-SCAN-011` Scan и preview можно отменить без оставшегося background work или
      принадлежащего им temp manifest.

## G. Архивы и безопасность результата

- [ ] `AC-ARC-001` ZIP, TAR.GZ и TAR.ZST создаются и читаются независимым reader.
- [ ] `AC-ARC-002` `include_root: true` создает `root/...`; `false` помещает внутрь
      только содержимое root.
- [ ] `AC-ARC-003` Empty directories и Unicode names сохраняются.
- [ ] `AC-ARC-004` Entry name никогда не абсолютный и не содержит traversal `..`.
- [ ] `AC-ARC-005` TAR сохраняет symlink, не читая target.
- [ ] `AC-ARC-006` ZIP сохраняет Unix-compatible symlink и возвращает warning о
      переносимости extraction.
- [ ] `AC-ARC-007` Непредставимый ZIP junction пропускается с явным warning, target
      не обходится, run не становится failed.
- [ ] `AC-ARC-008` Output и собственный temp/lock не включаются в архивируемый source.
- [ ] `AC-ARC-009` `skip` не изменяет существующий архив и не начинает запись.
- [ ] `AC-ARC-010` `increment` выбирает свободное имя без race в two-process test.
- [ ] `AC-ARC-011` `overwrite` сохраняет старый архив при любой ошибке до атомарной
      публикации нового.
- [ ] `AC-ARC-012` Temp создается на filesystem output; итоговый path резервируется
      между GUI/CLI процессами.
- [ ] `AC-ARC-013` Cleanup удаляет только temp/lock своего run.
- [ ] `AC-ARC-014` Stale reservation не удаляется без проверки владельца, возраста и
      отсутствия активного процесса.
- [ ] `AC-ARC-015` Обязательная structural verification обнаруживает поврежденный
      архив до публикации.
- [ ] `AC-ARC-016` Optional full verification полностью перечитывает архив.
- [ ] `AC-ARC-017` Optional checksum равен SHA-256 независимой утилиты; при
      `checksum: none` source/archive content hash не вычисляется.
- [ ] `AC-ARC-018` Fast/Balanced/Maximum используют mapping ADR-0004 для каждого
      формата.
- [ ] `AC-ARC-019` Disk-full, permission-denied и source mutation не оставляют
      частичный итоговый архив.
- [ ] `AC-ARC-020` Stop во время большого файла кооперативно прекращает run и
      безопасно очищает temp.

## H. Tasks, plan и defaults

- [ ] `AC-TASK-001` Выбранная папка представлена task card со source, profile,
      action, output, format/compression и status.
- [ ] `AC-TASK-002` Default output/archive settings автоматически применяются к новой
      задаче.
- [ ] `AC-TASK-003` Per-task override не меняет global default и может быть сброшен к
      default.
- [ ] `AC-TASK-004` Один активный `.packplan.yaml` атомарно автосохраняет tasks и
      overrides.
- [ ] `AC-TASK-005` Перезапуск восстанавливает task list, selected settings и enabled
      flags из plan.
- [ ] `AC-TASK-006` SQLite не является вторым источником истины для task list.
- [ ] `AC-TASK-007` В v1 UI не обещает Save As/Open/multi-plan.
- [ ] `AC-TASK-008` Task schema хранит ordered steps и может быть мигрирована к
      будущей цепочке действий.

## I. Scheduler и progress

- [ ] `AC-SCH-001` Scheduler соблюдает FIFO и `max_concurrent_tasks`.
- [ ] `AC-SCH-002` Run одной карточки не запускает другие queued tasks.
- [ ] `AC-SCH-003` Run all ставит все enabled tasks в очередь один раз.
- [ ] `AC-SCH-004` Pause заканчивает текущий entry и не начинает следующий.
- [ ] `AC-SCH-005` Paused run не потребляет CPU/не читает source, но занимает slot.
- [ ] `AC-SCH-006` Stop освобождает slot и запускает следующую queued task.
- [ ] `AC-SCH-007` Global pause блокирует старт queued runs и мягко приостанавливает
      активные.
- [ ] `AC-SCH-008` Global stop отменяет queued и кооперативно останавливает активные.
- [ ] `AC-SCH-009` Повторные pause/resume/stop идемпотентны.
- [ ] `AC-SCH-010` Недопустимый state transition возвращает typed error.
- [ ] `AC-SCH-011` Каждый run отправляет не более 10 progress events/sec.
- [ ] `AC-SCH-012` State, command acknowledgement, warning, error и final summary не
      задерживаются progress throttle.
- [ ] `AC-SCH-013` Per-task и global progress не показывают completion до
      фактического завершения writer/verification/publish.
- [ ] `AC-SCH-014` Reload webview не отменяет backend runs; snapshot восстанавливает
      очередь и состояния.

## J. История, logs и восстановление

- [ ] `AC-HIS-001` Одна task показывает несколько runs.
- [ ] `AC-HIS-002` Run detail содержит outcome, время, output, size, file counts,
      warnings, error и optional checksum.
- [ ] `AC-HIS-003` Status icon различает never-run, running/queued/paused,
      success, success-with-warnings, failed, cancelled и interrupted.
- [ ] `AC-HIS-004` Detailed logs загружаются страницами только при открытии.
- [ ] `AC-HIS-005` `Повторить` использует сохраненный settings/profile snapshot.
- [ ] `AC-HIS-006` `Запустить с текущими настройками` использует текущую task/profile,
      не меняя старый run.
- [ ] `AC-HIS-007` После crash незавершенный run становится `interrupted`, а не
      остается running.
- [ ] `AC-HIS-008` Run metadata очищается по более строгой границе один год/10 000.
- [ ] `AC-HIS-009` Logs очищаются по более строгой границе 90 дней/1 000 runs.
- [ ] `AC-HIS-010` `unlimited` отключает соответствующий retention.
- [ ] `AC-HIS-011` Очистка history/logs не удаляет архивы.
- [ ] `AC-HIS-012` `Открыть output folder` открывает только существующий
      валидированный path и ясно сообщает об отсутствующем.

## K. CLI

- [ ] `AC-CLI-001` Executable называется `foldry`, help/errors — на английском.
- [ ] `AC-CLI-002` Реализованы `profile list/show/create/edit/delete/validate`.
- [ ] `AC-CLI-003` Реализованы `preset list/install/remove`.
- [ ] `AC-CLI-004` Реализован `preview` с причиной решения.
- [ ] `AC-CLI-005` Реализован одиночный `archive`.
- [ ] `AC-CLI-006` Реализованы `plan validate/run`.
- [ ] `AC-CLI-007` Реализованы `history list/show` и `config show/path`.
- [ ] `AC-CLI-008` Human output пригоден для TTY; JSON output стабилен и не смешан с
      progress.
- [ ] `AC-CLI-009` Exit codes различают validation, I/O, partial success,
      cancellation и config errors.
- [ ] `AC-CLI-010` Ctrl+C использует общий cancellation path и не оставляет
      partial output.
- [ ] `AC-CLI-011` CLI и GUI одновременно не могут опубликовать один output path.

## L. GUI: общий shell и профильный режим

- [ ] `AC-GUI-001` GUI использует Mantine, CodeMirror 6 и Phosphor Icons.
- [ ] `AC-GUI-002` Light, dark и system themes применяются без перезапуска.
- [ ] `AC-GUI-003` Все пользовательские строки доступны на русском и английском;
      отсутствующий перевод использует английский fallback.
- [ ] `AC-GUI-004` Переключение locale изменяет даты/числа, но не CLI/machine values.
- [ ] `AC-GUI-005` Основные сценарии доступны с клавиатуры и имеют видимый focus.
- [ ] `AC-GUI-006` Статус не кодируется только цветом; reduced motion соблюдается.
- [ ] `AC-GUI-007` Profile mode имеет список слева, CodeMirror editor в центре,
      preset cards справа.
- [ ] `AC-GUI-008` Profile можно создать, переключить, переименовать, сохранить и
      удалить.
- [ ] `AC-GUI-009` Autosave включается/выключается в profile mode и показывает
      dirty/saving/saved/error.
- [ ] `AC-GUI-010` Autosave flush выполняется при переключении profile и закрытии.
- [ ] `AC-GUI-011` Syntax highlighting, line numbers и parser diagnostics работают
      для `.packignore`.
- [ ] `AC-GUI-012` Preset card показывает title, короткое описание, safe/sensitive и
      `absent|installed|modified|outdated`.
- [ ] `AC-GUI-013` Confirmation удаления modified preset находится внутри карточки.

## M. GUI: основной режим

- [ ] `AC-MAIN-001` Layout содержит filesystem tree слева, task cards в центре,
      selected task settings справа и global controls/progress снизу.
- [ ] `AC-MAIN-002` Есть `Выполнить все` и `Очистить выбор`.
- [ ] `AC-MAIN-003` Каждую task можно запустить из карточки и правой панели.
- [ ] `AC-MAIN-004` Во время run карточка показывает progress и доступные
      pause/resume/stop.
- [ ] `AC-MAIN-005` Есть global pause/resume/stop.
- [ ] `AC-MAIN-006` Queue position и concurrency limit видимы и настраиваются.
- [ ] `AC-MAIN-007` Default output и archive settings редактируются в modal и
      автосохраняются.
- [ ] `AC-MAIN-008` Per-task output/archive settings редактируются в modal.
- [ ] `AC-MAIN-009` Preview выбранной task виртуализирован, фильтруется и объясняет
      последнее правило.
- [ ] `AC-MAIN-010` Нажатие status открывает историю/log detail, не загружая все logs
      заранее.
- [ ] `AC-MAIN-011` Partial failure нескольких tasks ясно показывает отдельные и
      общий результаты.

## N. Persistence, privacy и security

- [ ] `AC-SEC-001` Config/data/cache paths получены системным API для
      `app.foldry.desktop`.
- [ ] `AC-SEC-002` Официального portable mode и записи рядом с executable нет.
- [ ] `AC-SEC-003` Settings, plan и profiles публикуются атомарно; fault injection
      не уничтожает предыдущую версию.
- [ ] `AC-SEC-004` Upgrade не перезаписывает вручную измененный profile/preset.
- [ ] `AC-SEC-005` Tauri capabilities минимальны; frontend не выполняет произвольные
      shell-команды и не читает произвольные файлы в обход commands.
- [ ] `AC-SEC-006` Все paths/DTO повторно валидируются Rust-стороной.
- [ ] `AC-SEC-007` Телеметрии и network crash upload endpoints нет.
- [ ] `AC-SEC-008` Crash reports локальны; history, logs, reports и recent paths
      очищаются отдельными действиями.
- [ ] `AC-SEC-009` Прямые contact/payment identifiers, credentials и содержимое
      secrets не попадают в logs.
- [ ] `AC-SEC-010` Archive entry construction защищено от path traversal.

## O. Linux

- [ ] `AC-LNX-001` MANUAL: native GUI/CLI устанавливаются и запускаются на
      поддерживаемом Linux.
- [ ] `AC-LNX-002` `/proc`, `/sys`, `/dev` показаны недоступными/специальными и не
      вызывают ошибочный рекурсивный обход.
- [ ] `AC-LNX-003` Symlink на файл и каталог сохраняется как link, target не
      обходится.
- [ ] `AC-LNX-004` Unreadable file/directory проверяет обе unreadable policies под
      непривилегированным пользователем.
- [ ] `AC-LNX-005` Mount и network mount визуально различимы; доступный mount внутри
      source обрабатывается согласно scanner contract.
- [ ] `AC-LNX-006` Atomic overwrite и two-process reservation проходят на локальном
      filesystem.

## P. Windows

- [ ] `AC-WIN-001` MANUAL: подписанный/тестовый installer и CLI устанавливаются и
      запускаются на поддерживаемой Windows.
- [ ] `AC-WIN-002` Browser показывает drives и Home; UNC/network path обрабатывается
      без UI freeze.
- [ ] `AC-WIN-003` Paths с Unicode, spaces и включенной long-path support
      архивируются.
- [ ] `AC-WIN-004` Symlink/junction не обходится; поддерживаемый link сохраняется.
- [ ] `AC-WIN-005` Непредставимый ZIP junction дает warning и не делает run failed.
- [ ] `AC-WIN-006` Locked file проверяет `fail` и `warn_and_skip`.
- [ ] `AC-WIN-007` Safe overwrite не удаляет старый архив, если Windows file
      replacement завершается ошибкой.
- [ ] `AC-WIN-008` Stop/pause работают при копировании большого файла.

## Q. macOS

- [ ] `AC-MAC-001` MANUAL: signed/notarized либо test bundle и CLI запускаются на
      поддерживаемой macOS.
- [ ] `AC-MAC-002` Browser показывает Home, `/` и mounted volumes без eager scan.
- [ ] `AC-MAC-003` APFS symlink сохраняется как link, target не обходится.
- [ ] `AC-MAC-004` Unicode normalization variants не создают неправильные duplicate
      entries или потерю файла.
- [ ] `AC-MAC-005` Permission-denied и privacy-protected directory дают понятную
      ошибку/недоступный node.
- [ ] `AC-MAC-006` Atomic overwrite и cleanup проходят на APFS.
- [ ] `AC-MAC-007` Light/dark/system theme следует системной теме.

## R. Документация и выпуск

- [x] `AC-DOC-001` `docs/how-it-works.md` простыми словами объясняет profiles,
      presets, tasks, preview, scheduler, архивы и history.
- [x] `AC-DOC-002` `docs/running.md` описывает prerequisites и dev/build/test для
      Linux, Windows и macOS.
- [x] `AC-DOC-003` Документированы `.packignore`, `.packplan.yaml`, settings, CLI и
      exit codes.
- [x] `AC-DOC-004` Документированы platform config/data/cache paths, retention,
      privacy и recovery.
- [ ] `AC-DOC-005` Release artifacts содержат installers/bundles, standalone CLI,
      checksums и SBOM.
- [ ] `AC-DOC-006` Clean install, upgrade и uninstall проверены; uninstall не удаляет
      пользовательские данные без явного действия.
- [x] `AC-DOC-007` Известные ограничения ZIP symlink/junction описаны рядом с
      выбором формата и в документации.

## Evidence журнала приемки

Для release candidate в этот раздел добавляется запись:

```text
Версия:
Commit:
Дата:
CI:
Linux manual:
Windows manual:
macOS manual:
Открытые отклонения:
Ответственный:
```

Текущий локальный кандидат:

```text
Версия: 0.1.0
Commit: локальное рабочее дерево; release commit ещё не создан
Дата: 2026-07-27
CI: workflow добавлен, удалённый run ещё не выполнялся
Linux manual: package build/layout и изолированный production launch пройдены
Windows manual: ожидается
macOS manual: ожидается
Открытые отклонения: notes/multiplatform-smoke.md, signing credentials, publish
Ответственный: владелец проекта и исполнитель OS-specific smoke
```

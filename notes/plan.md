# План реализации Foldry

## 1. Цель и принципы реализации

Нужно заново построить мультиплатформенное desktop-приложение и CLI для выполнения
действий над каталогами с едиными профилями фильтрации. Первое действие — создание
архивов ZIP, TAR.GZ и TAR.ZST.

План строится по следующим принципам:

- Rust-ядро не зависит от Tauri, React и SQLite.
- GUI и CLI используют один application API и одинаковые правила обработки.
- Каждый этап заканчивается работающим, проверяемым результатом.
- Форматы файлов и IPC-контракты версионируются с первой версии.
- Запись пользовательских YAML-файлов, профилей, БД и архивов выполняется атомарно.
- Сканирование и упаковка остаются ограниченными по памяти даже на очень больших
  каталогах.
- Новые типы действий добавляются через типизированный контракт, без переделки
  профилей, scheduler и истории.

Все известные на данный момент решения согласованы. Новые неоднозначности, если они
появятся при реализации, сначала записываются в `notes/decisions.md`; зафиксированные
ниже пункты являются принятыми рабочими контрактами.

## 2. Зафиксированные решения

### Продукт и этапность

- Название приложения — **Foldry**, application identifier —
  `app.foldry.desktop`. Название репозитория пользователь изменит отдельно.
- Rust packages именуются `foldry-*`, основной CLI executable — `foldry`. Alias `ab`
  не устанавливается.
- Разработка и приемка идут инкрементами от Rust core к CLI и GUI. Полная текущая
  цель — v1.1. Версия v1 содержит обязательные требования, v1.1 также включает
  полноценный CRUD пользовательских presets.
- GUI использует Mantine, CodeMirror 6 и Phosphor Icons.
- GUI с первой версии поддерживает русский и английский; английский — fallback.
  CLI, машинные error codes и JSON-поля остаются английскими.
- Доступная Linux-часть кроссплатформенного smoke принимается на этапе 14.
  Остальные практические Linux и все Windows/macOS проверки ведутся в
  `notes/multiplatform-smoke.md` и остаются обязательным release gate этапа 15.

### Хранение данных и форматы

- Официального portable mode нет. CLI получает overrides каталогов только для
  разработки, тестирования и восстановления, без обещания portable-дистрибутива.
- Рабочие settings, profiles и presets находятся в platform config directory;
  SQLite/history/logs — в platform data directory; временные manifests — в
  cache/temp directory.
- Поставляемые profile и preset-файлы при установке/первом запуске создаются как
  обычные редактируемые рабочие копии в config directory. В resources приложения
  хранится эталонный read-only набор presets для восстановления испорченных или
  удаленных копий. Пользовательские изменения нельзя молча перезаписывать при
  обновлении.
- В v1 существует один автоматически сохраняемый активный `.packplan.yaml`.
  SQLite не дублирует задачи: она хранит runtime runs/history/logs и ссылку на
  активный plan. Открытие, сохранение под новым именем и переключение нескольких
  plans откладываются на следующий этап развития.
- Один canonical source path соответствует одной верхнеуровневой задаче в plan.
  Повторный drop фокусирует существующую задачу. Задача содержит упорядоченный
  сценарий из action steps; в v1 доступен один step `archive`, но схема сразу
  допускает несколько шагов и новые типы действий.
- `.packignore` — UTF-8 текст. Профиль содержит `@profile-id` UUIDv7 и отображаемое
  `@profile-name`; rename не меняет ID. Default — обычный редактируемый файл, а не
  защищенный встроенный профиль, и может быть воссоздан из поставляемых presets.
- Форматы settings, plan, profile metadata, action specs и IPC DTO версионируются.
  Fixtures каждой выпущенной версии сохраняются для migration tests.

### Фильтрация и presets

- Семантика `.packignore` максимально совместима с `.gitignore`: последовательное
  применение и last-match-wins, Git-compatible pruning, normalized relative paths
  с `/`, case-sensitivity исходной файловой системы. Чтобы вернуть файл из
  исключенного каталога, профиль должен сначала вернуть родительский каталог.
  Полный синтаксис и отличия от Git документируются отдельно.
- Default исключает только воспроизводимые runtime/build/cache artifacts и VCS
  internals. Secrets, local config, database dumps и private media сохраняются.
- Чувствительные presets никогда не включаются автоматически, явно помечаются и
  показывают предупреждение перед вставкой.
- В v1 встроенные presets можно вставлять и удалять, но нельзя редактировать как
  самостоятельные сущности. Ручное изменение вставленного блока переводит его в
  `modified`. Полный CRUD пользовательских presets входит в v1.1.
- Autosave атомарно сохраняет и невалидный текст профиля, показывает diagnostics и
  помечает профиль `invalid`, запрещая новый run с ним. Перед записью сохраняется
  одна recoverable previous-good копия.

### Сканирование и архивирование

- Корневой каталог source по умолчанию включается в архив. В plan есть
  версионированная опция `include_root`, которую можно отключить.
- Symlink и junction никогда не обходятся. TAR сохраняет symlink нативно; ZIP
  записывает Unix-compatible symlink entry и warning о зависимости восстановления
  от extractor. Непредставимый в ZIP junction пропускается с явным warning; это не
  переводит run в ошибку и не зависит от `unreadable_policy`.
- Обычные файлы включаются, каталоги обходятся, special files пропускаются с warning.
  Для нечитаемого, исчезнувшего или изменившегося после planning файла default —
  `unreadable_policy: fail`; явная альтернатива — `warn_and_skip`. Успех с warnings
  визуально отличается от чистого успеха.
- Уровень сжатия хранится как `fast | balanced | maximum`; versioned mapping в
  параметры ZIP/gzip/zstd закрепляется в коде и документации. Точное codec level
  может быть advanced CLI option, но не показывается в обычном GUI.
- Output path резервируется между процессами атомарным sidecar/lock в output
  directory с `run_id`. Временный архив создается там же и может быть виден во время
  run. Stale artifacts удаляются только после проверки владельца. `overwrite`
  публикуется безопасной заменой, `skip` не начинает запись при конфликте,
  `increment` повторно разрешает гонку имени.
- Planning создает внутренний потоковый manifest: totals хранятся в памяти, entries
  — во временном файле. Формат manifest не является публичным.
- Перед публикацией всегда выполняется быстрая структурная проверка архива. Полный
  verify — опция задачи.
- Checksum итогового архива считается только при включенной настройке. Source content
  hashes не считаются. Служебные хеши текста profiles/presets для версий, cache и
  определения `modified/outdated` остаются частью системы.
- Reproducible archive mode учитывается в internal API, но его UI и полная реализация
  откладываются до версии после v1, если не появится отдельная необходимость.
- Preview показывает profile hash и время создания, инвалидируется при изменении
  профиля/source/action. Каждый run всегда строит новый execution plan.

### Scheduler, история и приватность

- Progress агрегируется до 10 событий в секунду на run. State changes, commands,
  errors и final result отправляются немедленно. Logs хранятся отдельно и читаются
  страницами только по запросу.
- Paused run сохраняет слот параллельности, но не потребляет CPU и не начинает новый
  entry. Stopped run освобождает слот.
- `Повторить` использует snapshot прошлого run, включая текст/hash профиля. Рядом
  доступно отдельное действие запуска с текущими настройками.
- Metadata runs хранится не более года и не более 10 000 последних запусков;
  подробные logs — не более 90 дней и не более 1 000 последних запусков. В обоих
  случаях действует более строгая граница, есть настройка `unlimited`. Очистка
  истории никогда не удаляет архивы.
- Корни дерева — Home, filesystem roots/drives и закрепленные пользователем места.
  Полное дерево доступно через filesystem root/drive и всегда загружается лениво.
- Телеметрии и сетевой отправки crash reports нет. Crash reports остаются только
  локально; пути не экспортируются наружу. Есть отдельная очистка истории и recent
  paths.
- Расширяемость actions обеспечивается версионированным `ActionSpec` и application
  handler interface. Неизвестный `type` сохраняется как unsupported там, где это
  безопасно. Динамический plugin ABI не проектируется до появления минимум двух
  новых реальных actions.

## 3. Целевая структура репозитория

Ориентировочная структура после bootstrap:

```text
foldry/
├── Cargo.toml
├── crates/
│   ├── foldry-core/          # профили, фильтрация, scanner, planner, архивы
│   ├── foldry-application/   # задачи, use cases, scheduler, settings, history ports
│   ├── foldry-storage/       # YAML/filesystem/SQLite adapters
│   ├── foldry-cli/           # английский CLI
│   └── foldry-tauri/         # Tauri commands, events, desktop integration
├── frontend/                 # React + TypeScript
│   ├── src/app/
│   ├── src/features/
│   ├── src/shared/
│   └── src/i18n/
├── resources/
│   ├── profiles/
│   └── presets/
├── tests/
│   └── fixtures/
├── docs/
└── notes/
```

Границы между crate окончательно закрепить архитектурными тестами и проверкой
dependency graph. Общие DTO не должны заставлять `foldry-core` зависеть от transport-
или storage-слоя.

## 4. Этапы

### Этап 0. Оформить принятые контракты

**Статус:** завершен 2026-07-26.

1. Использовать раздел 2 как источник принятых продуктовых и технических контрактов.
2. Новые неоднозначности сначала фиксировать в `notes/decisions.md`, после
   согласования переносить в раздел 2 и очищать из списка открытых вопросов.
3. На основе решений составить короткие ADR в `docs/architecture/` для форматов
   профиля/плана, хранения состояния, правил архивирования и scheduler.
4. Превратить требования в проверяемый acceptance checklist, включая отдельные
   сценарии Windows, macOS и Linux.

Результат: нет неоднозначности в поведении, которое влияет на форматы данных и
публичные API.

Артефакты этапа:

- `docs/architecture/README.md` и ADR-0001–ADR-0005;
- `docs/acceptance-checklist.md` с критериями v1/v1.1 и отдельными проверками Linux,
  Windows и macOS;
- `notes/decisions.md` без открытых вопросов.

### Этап 1. Очистить прототип и создать workspace

**Статус:** завершен 2026-07-26.

1. Перенести полезное содержимое `ignore.cfg` в поставляемую редактируемую копию
   профиля Default и подготовить эталонные presets для восстановления.
2. После переноса удалить Python/Tkinter-прототип, shell launcher и старые CFG-файлы;
   папку `notes/` не изменять и не удалять. Не затрагивать несвязанные пользовательские
   изменения до проверки diff.
3. Создать Rust workspace, React + TypeScript frontend и Tauri shell.
4. Настроить единый набор команд разработки: format, lint, typecheck, unit test,
   integration test и build.
5. Добавить базовый CI matrix для Linux, Windows и macOS с кэшированием Rust/npm.
6. Добавить политики качества: `rustfmt`, строгий `clippy`, TypeScript strict,
   ESLint/formatter, проверку YAML и Markdown.
7. Создать минимальные документы `README.md`, `CONTRIBUTING.md` и архитектурную карту.

Проверка этапа: пустое desktop-приложение, CLI и все crates собираются; CI выполняет
одинаковые проверки на трех ОС.

Артефакты этапа:

- Rust workspace из пяти crates, React/TypeScript/Mantine frontend и Tauri shell;
- поставляемый `resources/profiles/default.packignore` и эталонные presets,
  восстановленные из правил прототипа;
- единые команды `pnpm format:check`, `pnpm lint`, `pnpm typecheck`, `pnpm test`,
  `pnpm build`, `pnpm check` и `pnpm desktop:build`;
- `.github/workflows/ci.yml` с одинаковым quality gate для Linux, Windows и macOS;
- `README.md`, `CONTRIBUTING.md` и `docs/architecture/system-map.md`;
- успешно проверенные CLI bootstrap, frontend production build, Tauri release build
  и запуск desktop executable на Linux;
- browser-приемка bootstrap-интерфейса при 1440×900 и 390×844, включая light/dark
  theme, accessibility snapshot, отсутствие console errors и горизонтального
  переполнения.

### Этап 2. Описать доменные контракты и форматы данных

**Статус:** завершен 2026-07-27.

1. Ввести типы с явными идентификаторами: `ProfileId`, `PresetId`, `TaskId`, `RunId`,
   `PlanVersion`.
2. Описать типизированные модели:
   - профиль и диагностические сообщения парсера;
   - правило и результат совпадения с причиной;
   - задача, упорядоченный список action steps и версионированный `ActionSpec`;
   - `ArchiveActionSpec`, формат, уровень сжатия и conflict policy;
   - настройки приложения;
   - состояния задачи/run и события прогресса;
   - summary результата, warning и error taxonomy.
3. Определить версионированные схемы YAML settings и plan-файла.
4. Определить forward-compatible сериализацию каждого action step через поле `type`;
   в v1 валидировать единственный step `archive`, не меняя форму будущего сценария.
5. Добавить миграции форматов, строгую валидацию и понятные ошибки с путем до поля.
6. Сгенерировать TypeScript-типы transport DTO из Rust-контрактов либо проверять их
   на равенство в CI, чтобы GUI и backend не расходились.
7. Добавить golden-файлы для чтения, записи, неизвестных полей и будущих версий.

Проверка этапа: Rust round-trip тесты YAML и проверка сгенерированных TypeScript-
контрактов проходят; поврежденные и несовместимые файлы дают диагностические ошибки,
а не panic.

Артефакты этапа:

- UUIDv7 newtypes для profile/task/run и валидируемый slug-newtype для preset;
- типизированные profile/rule/match/archive/plan/settings/run/event/result контракты
  и стабильные warning/error taxonomies;
- YAML codec v1 для settings и plan с JSONPath-like ошибками, агрегированной
  валидацией и сохранением совместимых неизвестных полей;
- последовательный migration registry; версия 1 зафиксирована как первая выпущенная
  схема без предшествующей миграции;
- forward-compatible `ActionSpec`: неизвестный `type` сохраняется без потерь и
  блокирует выполнение, невалидный известный `archive` дает ошибку;
- Rust transport DTO, исчерпывающие domain-to-DTO преобразования и генерируемый
  `frontend/src/shared/contracts/generated.ts` с CI drift-check;
- golden, malformed, future-version, unknown-field/action и identifier fixtures в
  `tests/fixtures/formats/`;
- `docs/contracts/formats-v1.md` и `docs/contracts/transport.md`;
- успешно выполненный единый `pnpm check`, включая strict Clippy, TypeScript,
  frontend tests, Rust contract tests и workspace build.

### Этап 3. Реализовать профили фильтрации и пресеты

**Статус:** завершен 2026-07-27.

1. Реализовать parser максимально Git-compatible subset `.gitignore`:
   комментарии, escape для `#`/`!`, negation, anchored patterns, directory-only,
   `*`, `?`, `[]`, `**`, последовательное применение правил.
2. Для каждого правила сохранять source span: файл, номер строки, текст правила и
   идентификатор preset-блока.
3. Реализовать matcher, возвращающий не только include/exclude, но и последнее
   сработавшее правило для preview.
4. Реализовать метаданные профиля и стабильный ID, безопасные имена файлов и
   детектирование дубликатов ID.
5. Реализовать синтаксис preset-блоков, нормализацию и состояния
   `absent / installed / modified / outdated`.
6. Реализовать атомарные операции вставки, удаления и обновления preset-блока.
   Измененный блок не удалять без явно переданного подтверждения.
7. Подготовить поставляемые безопасные presets:
   Python, Node.js, Rust, Go, Java/Gradle/Maven, .NET, PHP/Composer, Ruby, C/C++,
   CMake, Django, React/Vite, Next.js, Vue, JetBrains, VS Code, macOS, Windows,
   Linux, test caches, coverage artifacts и build output.
8. Подготовить отдельные чувствительные presets с явной маркировкой и обязательным
   предупреждением перед вставкой:
   environment/secrets, local config, certificates/keys, database dumps,
   private media и deployment credentials.
9. Создать редактируемый Default из полезных правил текущего `ignore.cfg`, не включая
   чувствительные исключения; сохранить эталонный набор presets в resources.
10. Добавить table-driven, property и cross-platform path tests, включая negation
    внутри исключенных каталогов, Unicode и separator normalization.
11. Задокументировать полный синтаксис `.packignore`, Git-compatible pruning и все
    намеренные отличия от `.gitignore`.

Проверка этапа: один и тот же набор fixture-путей дает одинаковый результат на всех
ОС; для каждого результата доступна точная причина и строка профиля.

Артефакты этапа:

- parser метаданных, правил и preset-блоков с диагностикой, source spans и
  provenance каждого правила;
- Git-compatible matcher с last-match-wins, pruning исключенных каталогов,
  нормализацией separators, Unicode и настраиваемой case sensitivity;
- SHA-256 состояния preset-блоков `absent / installed / modified / outdated` и
  неизменяемые операции insert/update/remove с обязательными подтверждениями;
- каталог из 30 проверяемых resource-presets: 24 безопасных и 6 чувствительных,
  которые никогда автоматически не входят в Default;
- table-driven cross-platform fixtures и property tests для путей, negation,
  pruning, Unicode и нормализации;
- полный контракт `.packignore` в `docs/contracts/packignore-v1.md`;
- успешно выполненный единый `pnpm check`, включая форматирование, strict Clippy,
  drift-check контрактов, TypeScript, frontend/Rust tests и workspace build.

### Этап 4. Реализовать filesystem scanner и preview

**Статус:** завершен 2026-07-27.

1. Ввести платформенный слой классификации объектов: обычный файл, каталог,
   symlink, junction/reparse point, special file, unreadable entry, mount point.
2. Не следовать по symlink и junction; сохранять ссылку как отдельный entry.
3. Реализовать lazy API дерева файловой системы:
   - загрузка только прямых детей;
   - сортировка и стабильные IDs узлов;
   - признаки недоступности, symlink/junction, mount/network;
   - отмена устаревших запросов при сворачивании/смене каталога.
4. На Linux отдельно пометить `/proc`, `/sys`, `/dev` и недоступные каталоги;
   на Windows и macOS реализовать эквивалентное best-effort определение корней и
   специальных мест.
5. Отдавать Home, filesystem roots/drives и пользовательские favorites как корни
   дерева; полный filesystem остается доступен через соответствующий root/drive.
6. Реализовать сканирование action plan с bounded memory: summary хранится в памяти,
   а большой список entries — в потоковом/временном manifest.
7. Реализовать preview с порционной выдачей включенных и исключенных entries,
   фильтрацией по состоянию, причиной последнего совпадения, временем scan и hash
   профиля.
8. Инвалидировать preview cache при изменении профиля, source metadata или action;
   для каждого run всегда строить новый execution plan.
9. Добавить обработку циклов, исчезающих во время обхода файлов, permission errors,
   Unicode, длинных путей Windows и миллионов синтетических entries.

Проверка этапа: preview не блокирует UI, отменяется, не обходит ссылки и объясняет
каждое решение; memory benchmark не растет линейно от общего объема имен файлов.

Артефакты этапа:

- iterative scanner с классификацией file/directory/symlink/reparse/special/
  unreadable, обнаружением циклов и без обхода ссылок;
- read-only определение case sensitivity исходной файловой системы, mount/network
  flags и platform-special узлы Linux;
- lazy browser для Home, roots/drives, macOS volumes и favorites со стабильными
  node IDs, direct-children loading и поколениями отменяемых запросов;
- streaming manifest с bounded summary, безопасным ID, atomic ownership/cleanup и
  порционной фильтрацией по opaque byte cursor;
- preview cache key по точному тексту profile, source metadata и action, явная
  инвалидизация и transport DTO/TypeScript-типы browser/preview;
- integration tests для Unicode, matcher provenance, symlink, Unix special file,
  отмены, cleanup и end-to-end scanner → manifest → preview;
- synthetic-тест 1 000 000 manifest entries с фиксированным буфером 16 КиБ;
- контракт `docs/contracts/scanner-preview.md`;
- успешно выполненный единый `pnpm check`, включая полный workspace build.

### Этап 5. Реализовать planner и archive executor

**Статус:** завершен 2026-07-27.

1. Разделить создание immutable execution plan и его выполнение.
2. На этапе планирования:
   - проверить source/output и запретить архивирование результата в самого себя;
   - определить включенные entries, totals и warnings;
   - применить `include_root`, по умолчанию равный `true`;
   - выбрать свободный output path согласно conflict policy;
   - атомарно зарезервировать output path межпроцессным sidecar/lock с `run_id`;
   - создать временный файл в том же filesystem, что и итоговый архив.
3. Создать единый интерфейс archive writer и реализации ZIP, TAR.GZ, TAR.ZST.
4. Нормализовать UI-уровни `Fast / Balanced / Maximum` в настройки конкретного
   codec; CLI дополнительно может показывать точное отображение.
5. Сохранять каталоги, обычные файлы и symlink согласно принятому контракту;
   special files и непредставимые ZIP junction пропускать с warning.
6. Проверять pause между entries, cancellation также во время копирования больших
   файлов; не начинать новый файл после запроса pause.
7. Считать обработанные bytes/files и итоговый размер. Checksum итогового архива
   считать только при включенной настройке; source content hashes не считать.
8. Финализировать архив (`flush`/`finish`/`fsync` по выбранной политике), затем
   атомарно публиковать его:
   - `overwrite` безопасно заменяет старый архив только после успешного создания;
   - `skip` не запускает запись;
   - `increment` не имеет race между выбором имени и публикацией.
9. При ошибке/stop удалить только принадлежащий run временный файл и освободить
   reservation; существующий рабочий архив не трогать.
10. Перед публикацией выполнить быструю структурную проверку. При включенном full
    verify полностью перечитать архив.
11. Проверить созданные архивы независимыми readers, включая пустые каталоги,
    symlink, большие файлы, Unicode и повреждение source во время выполнения.

Проверка этапа: все форматы распаковываются ожидаемо; fault-injection тесты
подтверждают отсутствие частичных итоговых файлов и потери старого архива.

Артефакты этапа:

- immutable execution plan и streaming reader внутреннего manifest;
- межпроцессная sidecar reservation, `skip / overwrite / increment`, run-owned temp
  и cross-platform atomic publish;
- единый backend interface и writers ZIP, TAR.GZ, TAR.ZST с versioned codec mapping;
- сохранение directories/files/symlinks, typed warnings для ZIP links, junction,
  special и unreadable/changed entries;
- bounded spool для консистентного `fail / warn_and_skip`, pause на границе entry и
  stop между chunks большого файла;
- structural/full verification, optional SHA-256 и точные progress totals;
- independent-reader tests всех форматов, two-reservation race tests и
  fault-injection cleanup/overwrite tests;
- контракт `docs/contracts/archive-execution-v1.md`;
- успешно выполненный единый `pnpm check`, включая полный workspace build.

### Этап 6. Реализовать persistence и application services

**Статус:** завершен 2026-07-27.

1. Определить application ports для profile repository, settings repository,
   active plan repository, run history, logs и clock/ID generation.
2. Реализовать платформенные каталоги config/data/cache. Overrides разрешить для
   разработки/тестов, не создавая официальный portable mode.
3. Реализовать:
   - атомарное чтение/сохранение YAML settings;
   - атомарное чтение/сохранение единственного активного plan-файла;
   - profile/preset repositories с редактируемыми копиями в config и эталонными
     presets в resources;
   - SQLite migrations для runs, summaries, warnings/errors и logs.
4. Восстанавливать активный plan, задачи, per-task overrides и UI-независимые
   настройки после перезапуска.
5. Не считать оборванный процесс активным: при запуске reconciliation переводит
   незавершенные runs в `interrupted`, удаляет только подтвержденные stale temp/
   reservation artifacts.
6. Реализовать use cases: CRUD профилей, presets, settings, задач, preview, запуск,
   повтор запуска, получение истории и логов.
7. Реализовать retention: metadata runs — один год/10 000 запусков, logs —
   90 дней/1 000 запусков, с более строгой границей и настройкой `unlimited`.
8. Добавить конкурентные и crash-recovery integration tests.

Проверка этапа: состояние переживает перезапуск; старые версии схем мигрируют;
поврежденный пользовательский файл не перезаписывается молча и сопровождается
понятной диагностикой.

Артефакты этапа:

- application ports и storage-independent services для settings, active plan,
  profiles, presets, preview/run preparation, snapshot repeat, history и logs;
- platform-native config/data/cache directories с явными test/recovery overrides;
- атомарные YAML repositories, create-if-missing resources и previous-good профиль,
  включая сохранение и адресацию невалидного текста;
- SQLite schema v1 с транзакционными migrations, runs/summaries/warnings/errors/logs,
  пагинацией и UPSERT без каскадной потери logs;
- retention по возрасту и числу runs для metadata/logs с поддержкой `unlimited`;
- versioned reservation ownership metadata и безопасная startup reconciliation по
  PID, возрасту и точному имени run-owned temp-файла;
- integration tests перезапуска, конкурентной записи, migration rollback, retention,
  snapshot repeat, invalid profiles и crash cleanup;
- контракт `docs/contracts/persistence-v1.md`;
- успешно выполненный единый `pnpm check`, включая полный workspace build.

### Этап 7. Реализовать scheduler и управление run

**Статус:** завершен 2026-07-27.

1. Реализовать очередь с настраиваемым лимитом параллельности и стабильным порядком.
2. Сделать отдельную state machine с допустимыми переходами:
   `queued → planning → running ↔ paused → completed/failed/cancelled`, плюс
   `queued → cancelled`.
3. Реализовать запуск одной задачи, всех enabled-задач, повтор snapshot прошлого run
   и отдельный запуск с текущими настройками.
4. Реализовать per-task и global pause/resume/stop.
5. Paused run сохраняет свой слот параллельности, но не потребляет CPU и не начинает
   новый entry; stopped run освобождает слот и запускает следующий queued run.
6. Разделить event channels:
   - progress агрегируется и throttled до максимум 10 UI updates/sec;
   - state transitions, commands, errors и финальный summary отправляются сразу;
   - подробные logs сохраняются и читаются только по запросу.
7. Обеспечить идемпотентность повторных pause/stop, корректную конкуренцию команд и
   отсутствие зависших reservations.
8. Добавить deterministic tests с управляемыми clock/executor и stress tests очереди.

Проверка этапа: scheduler соблюдает лимит, порядок и все переходы при гонках команд;
частота progress не превышает настройку.

Артефакты этапа:

- отдельные `RunExecutor`, `RunReporter`, `RunEventSink` и scheduler ports без
  зависимости от CLI/Tauri;
- FIFO dispatcher с лимитом 1–64, явной state machine и сохранением каждой смены
  состояния в history;
- per-run/global pause, resume и stop с идемпотентными командами и освобождением
  слота после остановки;
- paused run удерживает слот и блокируется на общем execution checkpoint без начала
  следующего entry;
- строгая per-run последовательность немедленных state/warning/error/final events,
  monotonic progress throttle 100 мс и отдельная запись paged logs;
- current, all-enabled и historical snapshot-repeat preparation в application
  services;
- deterministic integration tests, 100-run stress queue и конкурентный
  pause/resume/stop test с проверкой SQLite final state;
- контракт `docs/contracts/scheduler-v1.md`;
- успешно выполненный единый `pnpm check`, включая полный workspace build.

### Этап 8. Сделать полноценный CLI

**Статус:** завершен 2026-07-27.

1. Создать executable `foldry` без alias `ab` и спроектировать английские команды
   как frontend к application services:
   - `profile list/show/create/edit/delete/validate`;
   - `preset list/install/remove`;
   - `preview`;
   - `archive`;
   - `plan validate/run`;
   - `history list/show`;
   - `config show/path`.
2. Поддержать human-readable и machine-readable JSON output.
3. Ввести документированные exit codes для validation, I/O, partial success,
   cancellation и config errors.
4. Добавить progress для TTY и спокойный line/JSON режим для CI.
5. Обеспечить обработку Ctrl+C через тот же cancellation path, что использует GUI.
6. Добавить snapshot и end-to-end тесты CLI на трех ОС.

Проверка этапа: архивирование и запуск plan полностью работают без GUI и используют
те же профили, planner, scheduler и историю.

Артефакты этапа:

- executable `foldry` с группами `profile`, `preset`, `preview`, `archive`, `plan`,
  `history` и `config`, без alias `ab`;
- общий runtime composition для platform directories, one-time resource copies,
  repositories, startup reconciliation и retention;
- production `ArchiveRunExecutor`, соединяющий snapshot → matcher → streaming
  manifest → reservation → writer/verify/publish с scheduler progress/logs;
- ZIP/TAR.GZ/TAR.ZST, semantic compression, `skip/overwrite/increment`,
  include-root, full verify и optional SHA-256 из CLI;
- human/TTY/quiet CI modes и versioned single-object JSON envelope;
- стабильные exit codes validation/I/O/partial/config/internal/cancelled;
- Ctrl+C через cooperative scheduler stop с удалением run-owned temp/reservation и
  сохранением существующего архива;
- end-to-end tests help/JSON/profile/preview/archive/history и Unix SIGINT; те же
  тесты без platform-specific сигнала входят в CI matrix Windows/macOS/Linux;
- документация `docs/cli.md`;
- успешно выполненный единый `pnpm check`, включая полный workspace build.

### Этап 9. Поднять безопасный Tauri IPC-слой

**Статус:** завершен 2026-07-27.

1. Экспортировать только application use cases, не внутренние core/storage типы.
2. Реализовать команды для папок, profiles/presets, tasks/settings, preview,
   scheduler, history/logs и desktop actions.
3. Валидировать все пути и входные DTO на Rust-стороне; не принимать произвольные
   shell-команды.
4. Реализовать подписки на state/progress events с correlation IDs и восстановление
   актуального snapshot после reconnect/reload frontend.
5. Реализовать нативные folder dialogs, drag-and-drop, open output folder и
   безопасные Tauri capabilities с минимальными разрешениями.
6. Добавить contract tests IPC и проверить, что frontend build использует свежие
   сгенерированные типы.

Проверка этапа: все операции GUI возможны только через типизированные команды;
перезагрузка webview не теряет backend-задачи и восстанавливает их состояния.

Артефакты этапа:

- runtime composition desktop-приложения с platform directories, resource copies,
  startup reconciliation, retention, application services, scheduler и archive
  executor;
- явный набор типизированных команд для bootstrap/reconnect, lazy browser,
  settings/plan/tasks, profiles/presets, preview, scheduler, history/logs и desktop
  actions;
- versioned desktop DTO и Rust → TypeScript генерация для bootstrap snapshot,
  profiles/presets, runs/logs, preview/browser correlation и стабильной IPC error
  envelope;
- `foldry://run-event` с упорядоченными per-run sequence и восстановлением актуальных
  scheduler/history/preview snapshots после reload;
- backend canonicalization и валидация путей, bounded paging/drop, UUIDv7 ID
  validation и reveal output исключительно через сохраненный `run_id`;
- нативные folder dialogs и output reveal через Rust plugin API без выдачи
  dialog/opener permissions webview;
- capability только `core:default`, отключенное автоматическое открытие ссылок,
  production CSP, frozen `Object.prototype` и явное отображение bundled resources;
- contract/runtime tests и контракт `docs/contracts/tauri-ipc-v1.md`;
- успешно выполненный единый `pnpm check`, включая полный workspace build.

### Этап 10. Реализовать frontend shell и дизайн-систему

**Статус:** завершен 2026-07-27.

1. Создать app shell, routing между основным режимом и редактором профилей.
2. Настроить Mantine, design tokens, light/dark/system theme, keyboard focus,
   reduced motion и Phosphor icons.
3. Настроить i18n с отсутствием строк, захардкоженных в компонентах.
4. Создать единый слой typed IPC/query cache и обработку backend errors.
5. Реализовать общие компоненты: split panels, cards, status badge, progress,
   confirmation-in-place, modal dialogs, empty/loading/error states.
6. Добавить component tests и визуальные сценарии для обеих тем и двух языков.

Проверка этапа: responsive desktop shell доступен с клавиатуры, переключает тему и
язык, корректно показывает loading/error/reconnect.

Артефакты этапа:

- responsive app shell с маршрутами Tasks/Profiles, широким трехпанельным режимом,
  компактными drawer-рейками и постоянной нижней панелью команд;
- Mantine theme tokens, light/dark/system color scheme, видимый keyboard focus,
  reduced-motion policy и Phosphor icons;
- EN/RU i18n provider с английским fallback и сохранением выбранного языка;
- типизированный `DesktopClient`, reconnect snapshot/query cache, прогресс по run,
  нормализованная обработка backend errors и browser preview adapter для UI-тестов;
- общие карточки, status/progress, loading/error/empty, drawer/modal и
  confirmation-in-place паттерны на базе общих Mantine tokens;
- component tests маршрутизации, typed snapshot, смены языка и темы;
- wide/compact visual concepts, эталонные rendered screenshots и fidelity ledger в
  `docs/design/`;
- keyboard/accessibility и responsive browser-приемка при 1440×900 и 820×760 без
  console errors или горизонтального переполнения;
- успешно выполненный единый `pnpm check`, включая frontend production build и
  полный Rust workspace; предупреждение о стартовом JS chunk 511 КБ перенесено в
  performance-проверки этапа 14.

### Этап 11. Реализовать редактор профилей

**Статус:** завершен 2026-07-27.

1. Собрать трехколоночный режим: profiles, CodeMirror 6 editor, preset cards.
2. Реализовать создание, переключение, переименование, сохранение и удаление
   профиля; Default редактируется и удаляется как обычный файл, но может быть
   воссоздан из поставляемых presets.
3. Показать dirty/saving/saved/error и включаемый autosave с debounce и flush при
   переключении/закрытии. Невалидный текст тоже сохранять атомарно, сохраняя одну
   previous-good копию и запрещая новые runs с invalid-профилем.
4. Добавить syntax highlighting, номера строк и diagnostics parser прямо в редактор.
5. Карточка preset показывает название, краткое описание, safe/sensitive и состояние
   `absent / installed / modified / outdated`.
6. Повторный клик добавляет/удаляет неизмененный preset; для modified подтверждение
   показывается внутри карточки; duplicate insert невозможен.
7. Добавить diff/preview перед обновлением outdated/modified preset.
8. Покрыть конфликт autosave, внешний edit файла, несохраненные изменения и ошибки
   записи тестами.

Проверка этапа: все операции профиля и presets работают без потери ручных правок;
невалидный текст сохраняется, диагностируется и не используется новым run.

Артефакты этапа:

- трехколоночный Profiles workspace: список профилей, CodeMirror 6 и каталог
  preset-карточек;
- backend use cases и Tauri IPC для создания/переименования профиля с UUIDv7,
  стабильным filename и валидацией имени; save/delete/restore Default используют
  существующее атомарное repository API;
- создание, переключение, rename, ручное сохранение, удаление и восстановление
  Default с модальными и confirmation-in-place сценариями;
- autosave с debounce, flush при переключении/unmount, состояниями
  dirty/saving/saved/error и защитой от потери более нового текста при завершении
  предыдущей записи;
- обнаружение внешнего изменения файла с явным выбором между локальным draft и
  дисковой версией;
- CodeMirror line numbers, подсветка metadata/preset/negation/comment, keyboard
  focus, lint gutter и backend parser diagnostics;
- состояния preset `absent / installed / modified / outdated`, защита от повторной
  вставки, явное подтверждение sensitive/modified и построчный diff перед заменой;
- lazy-loaded Profiles bundle, component/pure-state/Rust integration tests для CRUD,
  preset edits, autosave race и external-edit reconciliation;
- эталонный screenshot `docs/design/stage11-profile-editor-rendered.png` и свежая
  browser-приемка при 1440×900 без console errors;
- успешно выполненный единый `pnpm check`.

### Этап 12. Реализовать основной экран задач

**Статус:** завершен 2026-07-27.

1. Собрать layout:
   - слева lazy filesystem tree;
   - в центре task cards;
   - справа settings выбранной задачи;
   - снизу global settings/actions/progress.
2. Выбор каталога в дереве и drop одной/нескольких папок создают задачи; файлы
   игнорируются. Дубликат canonical source path фокусирует существующую задачу с
   ненавязчивым объяснением.
3. Карточка показывает source, action, profile, output, format/compression,
   состояние, отдельный progress и доступные run/pause/resume/stop controls.
4. Правая панель редактирует profile, упорядоченный сценарий action steps и task
   overrides; в v1 сценарий содержит один `archive`.
5. Модалки редактируют default output/archive settings и per-task overrides с
   возможностью вернуться к defaults.
6. Реализовать clear selection, run all, global pause/resume/stop и отображение
   queued-позиции.
7. Автосохранять active plan и настройки; запрещать случайное создание дублей по
   нормализованному/canonical пути согласно принятой политике.
8. Показывать недоступные filesystem nodes, symlink/junction и mount/network
   согласованными визуальными признаками, не полагаясь только на цвет.

Проверка этапа: полный пользовательский поток от выбора папок до управления очередью
работает мышью, drag-and-drop и клавиатурой и восстанавливается после перезапуска.

Артефакты этапа:

- трехколоночный Tasks workspace с lazy filesystem tree, карточками задач,
  прокручиваемым inspector и адаптивными drawers для компактной ширины;
- добавление каталога из дерева, native folder dialog и Tauri drag-and-drop, при
  этом файлы игнорируются, а canonical-дубликат фокусирует существующую задачу;
- полноценные task cards с профилем, output, format/compression, состоянием,
  прогрессом, queued-позицией и допустимыми run/pause/resume/stop действиями;
- autosave archive step и task overrides, включая profile, output/name, policies,
  include root и full verification;
- модалка default archive/output settings, clear selection и глобальные
  run/pause-or-resume/stop controls;
- доступные не только по цвету признаки unreadable, symlink/junction,
  mount/network узлов и отмена загрузки детей при сворачивании;
- component и pure-state tests для добавления задачи, смены состояния и archive
  defaults;
- эталонные screenshots `docs/design/stage12-tasks-wide-rendered.png` и
  `docs/design/stage12-tasks-compact-rendered.png`, browser-приемка при 1440×900 и
  820×760 без console errors;
- успешно выполненный единый `pnpm check`.

### Этап 13. Реализовать preview, историю и результат run в GUI

**Статус:** завершен 2026-07-27.

1. Для выбранной задачи показать paged/virtualized preview дерева или списка с
   include/exclude, поиском/фильтром и причиной решения.
2. Переиспользовать scanner cache только с явной инвалидизацией при изменении
   профиля, source metadata или настроек action; показывать время preview и hash
   профиля.
3. По клику на статус показать run history этой задачи.
4. Для run показать итог, время, файл, размер, количество файлов, warnings, error,
   действия `открыть output folder`, `повторить snapshot` и `запустить с текущими
   настройками`.
5. Загружать подробные logs только при открытии, с виртуализацией и экспортом в файл.
6. Агрегировать global progress без ложной точности, отдельно показывать queued,
   running, paused, succeeded и failed.
7. Добавить сценарии partial failure нескольких задач и сохранение результата после
   перезапуска.

Проверка этапа: причины фильтрации, история и ошибки доступны без загрузки гигантских
логов в память; обычный повтор использует snapshot прошлого run.

Артефакты этапа:

- lazy-loaded Run Explorer с вкладками Preview и Run history, открываемый отдельной
  кнопкой preview и кликом по статусу задачи;
- paged preview с include/exclude/skipped фильтрами, поиском по загруженной странице,
  виртуализированным списком, точной причиной/строкой/preset, временем scan и
  сокращенным hash профиля;
- отмена незавершенного scan при закрытии и переиспользование backend cache только
  после успешного preview; cache key уже инвалидируется профилем, metadata source и
  action;
- paged task history, ленивые `run_details` и результат с duration, counts, sizes,
  artifact, warnings/error и действиями reveal/repeat snapshot/run current;
- ленивые paged и виртуализированные logs и новый потоковый Tauri export в JSONL
  через native save dialog без удержания полного журнала в памяти;
- агрегированное состояние queued/running/paused/succeeded/failed и неопределенный
  общий progress вместо ложного процента при неизвестных totals;
- восстановление terminal-состояний карточек из persisted recent runs и browser
  fixtures для одновременного успеха с warnings и partial failure;
- unit/component tests для форматирования, preview и history; эталонные screenshots
  `docs/design/stage13-preview-wide-rendered.png`,
  `docs/design/stage13-history-wide-rendered.png` и
  `docs/design/stage13-preview-compact-rendered.png`;
- свежая browser-приемка при 1440×900 и 820×760 без console errors/warnings и
  успешно выполненный единый `pnpm check`.

### Этап 14. Hardening, производительность и кроссплатформенность

**Статус:** завершен 2026-07-27. Доступная Linux-часть принята; отложенная
практическая кроссплатформенная матрица ведётся как release gate этапа 15.

1. Провести threat/abuse review путей, symlink, archive path traversal, IPC
   capabilities и открытия внешних путей.
2. Запустить fuzzing parser профилей/YAML и property tests matcher/conflict naming.
3. Провести benchmarks scanner, matcher и writers на большом количестве маленьких
   файлов и больших файлах; зафиксировать регресс-пороги.
4. Проверить low-disk, permission denied, read-only output, исчезновение source,
   locked files Windows, длинные пути, Unicode normalization и сетевые диски.
5. Проверить crash recovery, атомарный overwrite, stale reservations и cleanup
   временных файлов.
6. Проверить локальные crash reports, отсутствие telemetry/сетевой отправки путей и
   работу очистки history/recent paths.
7. Выполнить accessibility audit и проверить масштабирование/HiDPI.
8. Провести ручной smoke test на поддерживаемых версиях Windows, macOS и Linux.

Проверка этапа: нет известных сценариев потери старого архива или тихого пропуска
данных; performance и resource budgets задокументированы.

Выполненная локальная часть:

- evidence-backed threat/abuse review зафиксирован в
  `docs/security/hardening/` и `docs/security/threat-review.md`; архитектурная
  переделка не потребовалась, два риска устранены в существующем executor boundary;
- execution manifest теперь повторно привязывается к source root перед каждым
  чтением, а обычные файлы открываются без перехода по подмененной symlink/reparse
  point с повторной проверкой identity/size/mtime;
- добавлены regression tests для выхода native path за source и scan-to-open
  symlink replacement, а также по 2048 generated-input случаев для parser, YAML и
  archive naming;
- release benchmark scanner, matcher и трех writers воспроизводится примером
  `performance_smoke`; Linux baseline, resource budget и регресс-пороги записаны в
  `docs/performance.md`;
- production dependency audit не нашел известных high-severity уязвимостей;
  runtime network/telemetry path в исходном коде и dependency graph не обнаружен;
- axe-core WCAG A/AA не нашел нарушений в light/dark desktop layout и compact
  layout 400×700; проверены reflow без горизонтального overflow, keyboard focus
  order и отсутствие browser console errors;
- автоматизированные fault/recovery тесты покрывают failed overwrite с сохранением
  старого архива, остановку и cleanup, атомарную публикацию, stale/live
  reservations, изменившийся/исчезнувший source, symlink и Unicode fixtures;
- `docs/platform-validation.md` фиксирует трехплатформенную CI matrix и оставшуюся
  ручную release matrix; полный `pnpm check` успешно выполнен после hardening.

Текущий Linux host не позволяет достоверно проверить native Windows/macOS dialogs
и packaging, Windows locked files/junctions/long paths, macOS
signing/notarization, HiDPI в реальном WebView, а также репрезентативные
network/removable drives и физический low-disk сценарий. Выполненные Linux-пункты
и оставшаяся release matrix зафиксированы в `notes/multiplatform-smoke.md`.

### Этап 15. Документация, packaging и выпуск

**Статус:** локальная реализация завершена 2026-07-27; финальная приемка и
публикация ожидают практическую матрицу из `notes/multiplatform-smoke.md`,
Windows/macOS signing/notarization credentials и удалённый release workflow run.

1. Написать `docs/how-it-works.md` простыми словами: profiles, presets, preview,
   tasks, scheduler, архивы, история и безопасность.
2. Написать `docs/running.md`: prerequisites, запуск dev/build/test на Windows,
   macOS и Linux.
3. Документировать синтаксис `.packignore`, формат plan/settings, CLI и exit codes.
4. Добавить troubleshooting и описание мест хранения config/data/logs.
5. Настроить Tauri bundles/installers с identifier `app.foldry.desktop`,
   application metadata/icons, signing и notarization там, где доступны credentials.
6. Собирать checksum/SBOM и release artifacts в CI; проверить clean install,
   upgrade и uninstall без удаления пользовательских данных.
7. Пройти acceptance checklist и выпустить первую стабильную версию.

Результат: устанавливаемые и документированные сборки Windows, macOS и Linux плюс
самостоятельный CLI.

Выполненная локальная часть:

- добавлены `docs/how-it-works.md`, `docs/running.md`,
  `docs/troubleshooting.md` и `docs/releasing.md`; актуализированы README, CLI,
  architecture map, platform validation и acceptance evidence;
- точные platform config/data/cache locations документированы, а слишком общий
  Linux-каталог `desktop` до выпуска заменён на Foldry-specific системный путь;
- Tauri bundling включён с product metadata, application identifier, полным
  набором icons, packaged Default profile/presets, Windows downgrade/WebView2
  policy и macOS hardened-runtime/minimum-version policy;
- release metadata drift проверяется `pnpm release:check` и входит в общий
  `pnpm check`;
- `.github/workflows/release.yml` повторяет quality gate, собирает native bundles
  и standalone CLI на Linux/Windows/macOS, формирует SPDX JSON SBOM и
  `SHA256SUMS`, загружает единый candidate artifact и создаёт только draft release;
- локально собраны `.deb`, `.rpm` и AppImage 0.1.0; проверены package metadata,
  icons, bundled resources и SHA-256; production desktop успешно запущен под
  `xvfb` с изолированными XDG-каталогами;
- после всех изменений успешно выполнен полный `pnpm check` с frontend 17/17,
  Rust unit/integration/property/security/recovery tests, strict lint/Clippy,
  contract drift, TypeScript и workspace production build;
- воспроизводимые debug/release caches очищены: освобождено 42,6 GiB, готовые
  package artifacts также не оставлены в рабочем дереве.

Оставшиеся release gates:

- выполнить clean install/upgrade/uninstall и native interaction checks на Linux;
- выполнить Windows и macOS пункты `notes/multiplatform-smoke.md`;
- настроить реальные code-signing/notarization credentials, проверить подписи;
- выполнить новый release workflow в удалённом CI, проверить собранные
  Windows/macOS/Linux assets, SBOM и checksums;
- закрыть оставшиеся пункты acceptance checklist и только после этого опубликовать
  стабильный release вместо draft.

## 5. Сквозная стратегия тестирования

- Unit: parser/matcher, naming, compression mapping, state machines, migrations.
- Golden/snapshot: YAML, profile diagnostics, CLI и IPC DTO.
- Property/fuzz: patterns, path normalization, serializations, conflict naming.
- Integration: scanner → planner → writer → independent extraction; SQLite recovery.
- Concurrency: scheduler, pause/stop races, reservations и event throttling.
- Frontend component: редактор, presets, task cards, dialogs, state/error states.
- End-to-end desktop: создание профиля, drop папки, preview, run, pause, stop,
  история, перезапуск.
- Cross-platform: реальные filesystem fixtures и архивы на Windows/macOS/Linux.
- Fault injection: read/write errors, disk full, crash перед/после publish.

## 6. Контрольные инкременты

1. **Core alpha** — этапы 1–5: профили, preview и безопасные архивы тестируются без UI.
2. **CLI alpha** — этапы 6–8: plan, очередь, история и все форматы доступны из CLI.
3. **Desktop alpha** — этапы 9–12: основной GUI и редактор профилей.
4. **Desktop beta** — этап 13: preview, история, logs и восстановление состояния.
5. **v1 release candidate** — этапы 14–15: hardening, документация и installers.
6. **v1.1** — CRUD пользовательских presets и согласованные улучшения после v1.

После каждого инкремента обновляются acceptance checklist, документация форматов и
миграционные тесты. Несогласованные идеи следующего инкремента не должны менять уже
выпущенный формат без явного повышения его версии.

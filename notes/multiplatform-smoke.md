# Ручная и практическая кроссплатформенная проверка Foldry

Этот документ — живой release checklist для проверок, которые зависят от реальной
операционной системы, WebView, filesystem и installer. Автоматизированные проверки
отмечаются отдельно: они снижают риск, но не заменяют native smoke.

Статусы:

- `[x]` — выполнено с указанным evidence;
- `[ ]` — ещё не выполнено;
- `N/A` — сценарий неприменим к платформе.

## Общая подготовка

На каждой ОС использовать временные каталоги без ценных данных и сохранить:

- версию Foldry, версию и архитектуру ОС;
- тип filesystem для source и output;
- точные шаги и экспорт logs при ошибке;
- наличие старого архива, `.part` и reservation sidecar после завершения;
- результат независимого открытия/распаковки архива.

Для destructive-сценариев не использовать `overwrite` с ценными архивами.

## Linux

Проверено 2026-07-27 на текущем Linux x86-64 host:

- [x] Полный workspace quality gate: format, lint/Clippy, contracts, TypeScript,
      frontend/Rust tests и production build — `corepack pnpm check`.
- [x] Desktop executable собирается и запускается; основной shell проверен на
      wide/compact viewport — этапы 1, 10 и 12 в `notes/plan.md`.
- [x] ZIP, TAR.GZ и TAR.ZST создаются, проходят structural/full verification и
      читаются независимыми readers — `archive_execution` и `archive_writers`.
- [x] Pause/resume/stop, остановка во время чтения и cleanup run-owned temp —
      `scheduler_runtime`, `archive_execution` и CLI end-to-end tests.
- [x] Ошибка до публикации сохраняет старый архив; `skip`, `increment`,
      atomic overwrite и stale/live reservations — `output_reservation` и
      `startup_reconciliation`.
- [x] Исчезнувший/изменившийся source обрабатывается по выбранной policy; подмена
      файла symlink после scan не приводит к чтению target — `archive_execution`.
- [x] Symlink не обходится, special files пропускаются с typed warning —
      `filesystem_scanner`.
- [x] Unicode paths/names и platform case behavior покрыты fixtures —
      `profile_compatibility`, matcher и archive tests.
- [x] Light/dark WCAG A/AA, keyboard order и reflow 400×700 без горизонтального
      overflow проверены axe-core/Playwright; browser console чиста.
- [x] Release benchmark scanner/matcher/writers и peak RSS записаны в
      `docs/performance.md`.
- [x] Production desktop и Linux bundles `.deb`, `.rpm`, AppImage собраны; package
      metadata/icons/resources проверены, production binary запущен под `xvfb` с
      изолированными XDG-каталогами и создал Foldry-specific config/data.
- [ ] Проверить native folder dialog и реальный desktop drag-and-drop в
      release bundle.
- [ ] Проверить read-only/permission-denied output под непривилегированным
      пользователем на ext4 и убедиться, что старый архив сохранён.
- [ ] Проверить физический `ENOSPC` на ограниченном loopback/tmpfs filesystem.
- [ ] Проверить output на реальном network mount и removable drive.
- [ ] Проверить packaged AppImage/Debian/RPM bundle на чистой системе: install,
      upgrade, uninstall и сохранение пользовательских данных.
- [ ] Проверить native UI при 200% scaling/HiDPI.

## Windows

Выполнить на поддерживаемой Windows x64 с WebView2:

- [ ] Установить подписанный или тестовый MSI/NSIS bundle на чистую систему и
      запустить Foldry.
- [ ] Проверить native folder dialog, выбор нескольких папок и drag-and-drop;
      dropped file должен быть отклонён с объяснением.
- [ ] Создать ZIP, TAR.GZ и TAR.ZST, открыть output folder и независимо распаковать
      каждый архив.
- [ ] Проверить pause/resume/stop и восстановление interrupted run после
      принудительного завершения процесса.
- [ ] Проверить read-only/permission-denied output и сохранение старого архива.
- [ ] Удерживать source file открытым с exclusive lock и проверить `fail` и
      `warn_and_skip`.
- [ ] Включить long paths и проверить глубокий Unicode path длиннее 260 символов.
- [ ] Проверить symlink, directory junction и reparse point: target не обходится.
- [ ] Проверить local NTFS, UNC/network share и removable drive.
- [ ] Проверить light/dark, keyboard-only flow и 200% scaling/HiDPI.
- [ ] Проверить clean install, upgrade и uninstall; config/data/history пользователя
      после upgrade и uninstall не должны удаляться автоматически.
- [ ] Проверить publisher/signature и отсутствие неожиданных SmartScreen/package
      ошибок для release artifact.

## macOS

Выполнить на поддерживаемой macOS Apple Silicon; x64 проверить отдельно, если такая
сборка публикуется:

- [ ] Установить notarized DMG/app bundle на чистую систему и пройти первый запуск
      через Gatekeeper.
- [ ] Проверить native folder dialog, sandbox permissions и drag-and-drop нескольких
      каталогов.
- [ ] Создать ZIP, TAR.GZ и TAR.ZST, выполнить reveal и независимо распаковать
      каждый архив.
- [ ] Проверить pause/resume/stop и восстановление interrupted run после
      принудительного завершения приложения.
- [ ] Проверить read-only/permission-denied output и сохранение старого архива.
- [ ] Проверить исчезновение source и изменение файла во время run для обеих
      unreadable policies.
- [ ] Проверить symlink без traversal и Unicode normalization на APFS, включая
      визуально одинаковые NFC/NFD names.
- [ ] Проверить local APFS, SMB/network mount и removable drive.
- [ ] Проверить light/dark, keyboard-only flow, Retina/200% scaling.
- [ ] Проверить подпись `.app`, hardened runtime, notarization ticket и отсутствие
      Gatekeeper warnings.
- [ ] Проверить clean install, upgrade и удаление приложения без автоматического
      удаления config/data/history пользователя.

## Release gate

Linux считается закрытым для этапа 14 в объёме доступных локальных и
автоматизированных проверок. Открытые практические Linux-пункты и вся Windows/macOS
матрица остаются обязательным release gate этапа 15. Релиз нельзя считать
полностью кроссплатформенно проверенным, пока эти пункты не выполнены либо явно не
сняты отдельным решением.

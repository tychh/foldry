# ADR-0004: Сканирование и безопасное создание архивов

- Статус: Accepted
- Дата: 2026-07-26

## Контекст

Foldry должен объяснять результат фильтрации, работать на больших деревьях и не
терять существующий архив при ошибке. Filesystem behavior отличается между Linux,
Windows и macOS, особенно для symlink, junction, permissions и atomic replace.

## Решение

### Filesystem scanner

- Native path используется для I/O; normalized relative path с `/` — только для
  matching и archive entry name.
- Обычный файл включается, каталог обходится.
- Symlink и Windows junction/reparse point никогда не обходятся.
- Mount/network directory визуально помечается, но обходится как обычный каталог,
  если доступен и попадает внутрь выбранного source.
- Special file пропускается с warning.
- Нечитаемый, исчезнувший или изменившийся после planning файл:
  - default `unreadable_policy: fail`;
  - optional `warn_and_skip`;
  - в обоих случаях событие попадает в summary/log.
- На Linux `/proc`, `/sys`, `/dev` и unreadable nodes помечаются как недоступные в
  browser. Эквивалентные platform-specific узлы обрабатываются best effort.
- Дерево GUI загружает только direct children. Корни: Home, roots/drives и favorites.
- Scan поддерживает cooperative cancellation.

### Preview и execution plan

- Preview выдается страницами и содержит include/exclude плюс provenance последнего
  правила.
- Preview показывает время и profile hash.
- Cache инвалидируется при изменении profile, action или наблюдаемой source metadata.
- Run никогда не использует preview как авторитетный snapshot: он строит новый
  immutable execution plan.
- Planner считает totals, но entries записывает во внутренний временный streaming
  manifest. Публичной совместимости у manifest нет.
- Если source меняется между planning и чтением entry, применяется
  `unreadable_policy`, а расхождение фиксируется.

### Archive layout и formats

- Форматы v1: ZIP, TAR.GZ, TAR.ZST.
- По умолчанию archive entries начинаются с имени source root.
- `include_root: false` архивирует только содержимое source.
- Absolute paths, `..` и platform prefixes никогда не попадают в entry name.
- Empty directories сохраняются.
- TAR сохраняет symlink нативно.
- ZIP использует Unix-compatible symlink entry и добавляет warning о зависимости
  восстановления от extractor.
- Непредставимый ZIP junction не обходится, пропускается с явным warning и не
  переводит run в ошибку.
- Reproducible mode учитывается в internal API, но не реализуется полностью до
  версии после v1.

### Compression

Plan хранит semantic level. Mapping version 1:

| Формат/codec | Fast | Balanced | Maximum |
| ------------ | ---: | -------: | ------: |
| ZIP/DEFLATE  |    1 |        6 |       9 |
| TAR.GZ/gzip  |    1 |        6 |       9 |
| TAR.ZST/zstd |    1 |        3 |      19 |

Изменение mapping требует новой mapping version и migration/default policy.

### Output reservation и публикация

1. Проверить source/output и исключить итоговый/временный архив из source.
2. Разрешить conflict policy `overwrite`, `skip` или `increment`.
3. Межпроцессно зарезервировать итоговый path атомарным sidecar/lock с `run_id`.
4. Создать уникальный временный архив в том же filesystem, что и output.
5. Записать entries и завершить codec.
6. Выполнить обязательную быструю структурную проверку.
7. При `verify: full` полностью перечитать и проверить архив.
8. При включенном checksum вычислить SHA-256 итогового архивного файла. Source
   content hashes не вычисляются.
9. Атомарно опубликовать temp:
   - `overwrite` заменяет старый файл только после успешного завершения temp;
   - `skip` не начинает запись, если target зарезервирован/существует;
   - `increment` повторяет выбор имени при конкурентном конфликте.
10. Освободить reservation.

При fail/stop Foldry удаляет только собственные temp/lock текущего run. Ранее
существовавший архив остается нетронутым. Stale lock удаляется только после проверки
владельца, возраста и отсутствия активного процесса.

### Pause и stop на уровне executor

- Pause становится активной после завершения текущего entry; новый entry не
  открывается.
- Во время pause задача не читает source и не потребляет CPU.
- Stop проверяется также между chunks большого файла, прекращает работу
  кооперативно и запускает безопасный cleanup.

## Последствия

- Planning делает дополнительный проход и использует temporary storage, но дает
  totals, объяснимость и bounded memory.
- ZIP links не полностью переносимы, поэтому warnings являются частью результата.
- Temp рядом с output может быть видим пользователю во время run.
- Full verify и maximum compression могут существенно увеличить время выполнения.

## Проверка

- Архивы открываются независимыми readers на трех ОС.
- Fault injection до/после каждого шага публикации не повреждает старый архив.
- Two-process tests подтверждают отсутствие гонки `increment/overwrite`.
- Scanner tests покрывают Unicode, long paths, links, permissions, mounts и
  исчезновение файлов.
- Benchmark на большом дереве подтверждает отсутствие списка всех entries в RAM.

## История

- 2026-07-26 — решение принято для начала реализации.

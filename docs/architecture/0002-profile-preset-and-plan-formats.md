# ADR-0002: Форматы profiles, presets, settings и plan

- Статус: Accepted
- Дата: 2026-07-26

## Контекст

Profiles и plan должны оставаться понятными текстовыми файлами, редактироваться
внешними редакторами и одинаково использоваться GUI/CLI. Одновременно нужны
стабильные IDs, диагностика по строкам, autosave, presets и возможность развивать
actions без неявного изменения старых файлов.

## Решение

### Profile

- Расширение: `.packignore`.
- Кодировка: UTF-8. При записи Foldry использует LF и завершающий newline.
- Обязательные метаданные являются комментариями:

```text
# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
# @profile-version 1
# @profile-name Default
```

- `profile-id` — UUIDv7 и не меняется при rename.
- Имя файла — безопасный slug для человека, но не идентификатор.
- Дубликат ID является диагностической ошибкой repository.
- Default — обычный редактируемый profile и может быть удален. Действие
  восстановления собирает его заново из поставляемых presets.

### Правила фильтрации

- Синтаксис максимально совместим с `.gitignore`: comments/escaping, negation,
  anchored patterns, directory-only, `*`, `?`, `[]`, `**`.
- Правила применяются сверху вниз; последнее совпадение определяет результат.
- Для matching native relative path нормализуется к separator `/`, но I/O всегда
  использует исходный native path.
- Case-sensitivity соответствует исходной файловой системе.
- Действует Git-compatible pruning: для re-include entry из исключенного каталога
  профиль должен сначала re-include родительский каталог.
- Matcher возвращает решение и provenance: profile ID, номер строки, исходное
  правило и preset ID, если правило находится в preset-блоке.

Полная пользовательская спецификация синтаксиса создается до завершения этапа 3.
Compatibility fixtures являются частью публичного контракта.

### Preset-блок

В profile preset представлен явно размеченным блоком:

```text
# @preset-begin id=python version=1
__pycache__/
*.py[cod]
# @preset-end id=python
```

- Повторный блок с тем же ID запрещен.
- Для сравнения нормализуются line endings и наличие последнего newline; marker-
  строки не входят в hash. Остальные пробелы и порядок правил значимы.
- Каталог поставляемых presets хранит текущую версию и hashes известных прошлых
  версий.
- Состояния имеют следующий приоритет:
  - `absent` — корректного блока нет;
  - `modified` — блок есть, но content hash не совпадает с объявленной известной
    версией;
  - `outdated` — блок не изменен, но существует более новая поставляемая версия;
  - `installed` — блок не изменен и соответствует текущей версии.
- Измененный блок нельзя вставить повторно. Удаление требует inline confirmation.
- Sensitive preset явно маркируется в каталоге, никогда не устанавливается
  автоматически и требует warning перед вставкой.
- В v1 поставляемые presets вставляются/удаляются, но CRUD самостоятельных
  пользовательских presets появляется в v1.1.

### Невалидный profile

- Autosave атомарно сохраняет и невалидный текст, чтобы не терять ручную работу.
- Перед заменой сохраняется одна recoverable previous-good копия.
- Parser diagnostics видны в редакторе.
- Невалидный profile нельзя назначить новому run. Существующая задача остается в
  plan и показывает ошибку.

### Settings

- Формат: versioned YAML.
- Неизвестная будущая major version не выполняется и дает понятную ошибку.
- Неизвестные поля совместимой версии сохраняются при round-trip, если выбранная
  библиотека позволяет сделать это безопасно; иначе запись блокируется, чтобы не
  потерять данные.
- Уровень сжатия хранится семантически: `fast`, `balanced`, `maximum`.
- GUI locale: `ru` или `en`, fallback — `en`. CLI и machine-readable output —
  английские.

### Plan

- Расширение: `.packplan.yaml`.
- В v1 существует один активный автоматически сохраняемый plan.
- Plan — единственный источник истины для настроенных задач; SQLite их не дублирует.
- Минимальная форма:

```yaml
version: 1
name: Active plan

tasks:
  - id: 0190f5f0-7f8b-7d80-a120-4f4f9fe95c21
    source: /home/user/projects/example
    enabled: true
    profile_id: 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
    steps:
      - type: archive
        output:
          directory: /home/user/Packages
          filename: example-{date}
          format: zip
          compression: balanced
          conflict_policy: increment
        include_root: true
        unreadable_policy: fail
        verification:
          mode: structural
          checksum: none
```

- Task ID стабилен. В пределах plan разрешена одна задача на canonical source path.
- Повторный drag-and-drop фокусирует существующую задачу.
- `steps` — упорядоченный сценарий. v1 валидирует один `archive`, но форма допускает
  будущие последовательности.
- Default settings подставляются application layer. Сохраненный plan явно хранит
  эффективные task overrides, необходимые для воспроизводимости.
- Запись settings, plan и profile всегда атомарна.

### Версионирование

- Settings, plan, profile metadata, preset catalog, action specs и IPC DTO имеют
  версии.
- Для каждой выпущенной версии сохраняются read/migrate fixtures.
- Изменение смысла существующего поля требует migration или новой major version.

## Последствия

- Файлы остаются пригодными для обычных редакторов и source control.
- UUID отделяет идентичность от filename/display name.
- Сохранение неизвестных данных требует round-trip-aware YAML adapter.
- Поддержка provenance увеличивает модель rule, но делает preview объяснимым.

## Проверка

- Golden round-trip tests покрывают valid/invalid/future files.
- Compatibility tests проверяют `.gitignore`-подобные patterns на трех ОС.
- Profile diagnostics всегда содержат строку и исходное правило.
- Duplicate IDs и duplicate canonical source paths не сохраняются молча.

## История

- 2026-07-26 — решение принято для начала реализации.
- 2026-07-27 — уточнено явное поле версии profile metadata; добавлена ссылка на
  реализованную спецификацию v1.

Реализованная схема и compatibility behavior описаны в
[`docs/contracts/formats-v1.md`](../contracts/formats-v1.md).

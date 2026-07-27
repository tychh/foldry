# ADR-0001: Границы системы и зависимости

- Статус: Accepted
- Дата: 2026-07-26

## Контекст

Foldry должен иметь React/Tauri GUI и самостоятельный CLI, но одинаково обходить
файлы, применять profiles, создавать архивы и управлять задачами. В дальнейшем
появятся новые действия и последовательности действий. Связывание core с Tauri,
React или SQLite сделало бы CLI вторичным и усложнило расширение.

## Решение

### Идентичность продукта

- Product name: `Foldry`.
- Application identifier: `app.foldry.desktop`.
- Rust packages: `foldry-*`.
- CLI executable: `foldry`; alias `ab` не устанавливается.

### Слои

Направление зависимостей:

```text
React GUI -> Tauri adapter ─┐
                           ├─> application -> core
CLI adapter ───────────────┘        │
                                   └─> storage ports
                                            ▲
                                      storage adapters
```

- `foldry-core` содержит profiles, matcher, filesystem scanner, planner, archive
  writers и доменные ошибки. Он не зависит от Tauri, React, SQLite или конкретного
  CLI.
- `foldry-application` содержит use cases, tasks, scheduler, settings/history ports
  и orchestration. Он зависит от core, но не от GUI.
- `foldry-storage` реализует filesystem/YAML/SQLite adapters.
- `foldry-cli` и `foldry-tauri` являются равноправными transport adapters к одним
  application services.
- React получает только transport DTO. TypeScript DTO генерируются из Rust-
  контрактов; CI запрещает незакоммиченный drift.

### Расширение действий

- Верхнеуровневая задача привязана к одному canonical source path.
- Задача содержит упорядоченный список `ActionSpec`.
- В v1 разрешен один action step типа `archive`.
- Неизвестный `type` читается и показывается как unsupported, если его можно
  безопасно сохранить, но не выполняется.
- Динамический plugin ABI не создается до появления как минимум двух новых реальных
  actions. Расширяемость пока обеспечивают versioned enum/DTO и handler interface.

### Граница релизов

- Разработка принимается инкрементами Core alpha, CLI alpha, Desktop alpha, Desktop
  beta и v1 release candidate.
- v1 реализует обязательные требования.
- Текущая полная цель v1.1 дополнительно включает CRUD пользовательских presets.

## Последствия

- Core и application можно тестировать без webview и SQLite.
- GUI не имеет скрытой бизнес-логики, которой нет в CLI.
- Добавление action требует нового spec/handler и UI, но не изменения profile
  matcher.
- Межслойные DTO придется явно версионировать и мигрировать.

## Проверка

- Dependency graph не содержит ссылок из `foldry-core` на Tauri/SQLite/frontend.
- Один integration fixture дает одинаковый план архива через CLI и Tauri.
- CI генерирует TypeScript DTO и проверяет чистый diff.

## История

- 2026-07-26 — решение принято для начала реализации.

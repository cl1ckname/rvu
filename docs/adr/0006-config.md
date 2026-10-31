# ADR-0006: Конфиг в TOML по XDG-пути

**Статус:** принято · 2026-10-31

## Решение

- Файл `$XDG_CONFIG_HOME/rvu/config.toml` (по умолчанию `~/.config/rvu/`).
  Пути — через крейт `dirs` (на Linux следует XDG: `config_dir`, `state_dir`).
- Переопределение: флаг `--config` или переменная `RVU_CONFIG`.
- `serde` + `toml`, `#[serde(default, deny_unknown_fields)]`: файл не
  обязателен, все поля имеют дефолты, опечатки — ошибка.
- Секреты: токен GitLab можно не хранить в файле (`token_env`,
  `token_command`).

Формат описан в [config.md](../config.md).

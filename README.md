# rvu

Терминальный клиент для ревью кода: локальный `git diff` и merge request'ы
GitLab, комментарии прямо на просматриваемой строке, клавиши в стиле nvim.

```sh
rvu                                   # ревью `git diff` в текущем репозитории
rvu https://gitlab.example.com/group/project/-/merge_requests/42
```

## Документация

- [Видение продукта](docs/vision.md)
- [Roadmap](docs/roadmap.md)
- [Архитектура](docs/architecture.md)
- [Клавиши](docs/keybindings.md)
- [Конфигурация](docs/config.md)
- [Архитектурные решения (ADR)](docs/adr/)

## Разработка

`~/.local/bin/rvu` — симлинк на `target/debug/rvu`: после `cargo build`
свежая версия сразу доступна в любом репозитории.

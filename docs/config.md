# Конфигурация

## Расположение

`$XDG_CONFIG_HOME/rvu/config.toml`, если `XDG_CONFIG_HOME` не задан —
`~/.config/rvu/config.toml`.

Путь можно переопределить флагом `--config <path>` или переменной `RVU_CONFIG`.

Файл не обязателен: без него используются значения по умолчанию.
Шаблон со всеми настройками и их значениями по умолчанию —
[`assets/config.toml`](../assets/config.toml) (тест следит, чтобы он
совпадал с дефолтами в коде). Начать можно с копии:

```sh
mkdir -p ~/.config/rvu && cp assets/config.toml ~/.config/rvu/config.toml
```
Неизвестные ключи — ошибка (чтобы опечатки не терялись молча).

Логи пишутся в `$XDG_STATE_HOME/rvu/rvu.log` (`~/.local/state/rvu/rvu.log`).

## Пример

```toml
[ui]
# Режим при запуске: "scroll" | "page"
default_mode = "scroll"
# Тема подсветки — любая тема bat: "base16-ocean.dark", "Catppuccin Mocha",
# "gruvbox-dark", "Nord", "OneHalfDark", "TwoDark", "Dracula", ...
# При неизвестном имени rvu выведет список доступных.
theme = "base16-ocean.dark"
# Показывать дерево файлов при запуске (во время работы — Ctrl-b)
show_tree = true
# Ширина панели дерева файлов в колонках
tree_width = 30
# Строк контекста вокруг ханка в режиме Scroll
context_lines = 3

[page]
# Сворачивать неизменённые участки в режиме Page при запуске.
# Во время работы: `zi` — переключить, `zM` — свернуть, `zR` — развернуть.
fold_unchanged = false
# Сколько неизменённых строк оставлять над и под каждым изменением.
fold_context = 5

# Переопределение синтаксиса комментариев по расширению или имени файла
# (для файлов без расширения: Justfile, Tiltfile).
# Строка — однострочный комментарий, массив из двух строк — блочный.
# Приоритетнее встроенной таблицы (src/comment.rs, ~130 языков).
[comments.syntax]
rs = "//"
sql = "--"
vue = ["<!--", "-->"]

# GitLab-инстансы. Инстанс выбирается по хосту из URL MR.
[[gitlab]]
host = "gitlab.example.com"
# Способ получения токена — ровно один из трёх:
# token = "glpat-..."              # прямо в файле (не рекомендуется)
token_env = "RVU_GITLAB_TOKEN"     # из переменной окружения
# token_command = "pass show work/gitlab-token"   # вывод команды (stdout, trim)
```

## Токен GitLab

Нужен Personal Access Token со scope `api` (`read_api` хватит только для
просмотра без комментариев).

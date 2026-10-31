# ADR-0004: GitLab MR целиком через REST API

**Статус:** принято · 2026-10-31

## Контекст

Основная цель — self-hosted GitLab, авторизация по Personal Access Token.

**Поддерживаемые версии: GitLab 15.7+.** С этой версии есть
`/merge_requests/:iid/diffs`; устаревший `/changes` (обрезает большие MR)
не используем и запасной вариант для старых версий не делаем.

## Решение

Свой тонкий клиент на `reqwest` + `serde` (готовый крейт `gitlab` избыточен,
нужно около десятка эндпоинтов). Всё — через API, без локального клона:

| Задача                  | Эндпоинт |
|-------------------------|----------|
| MR, `diff_refs`         | `GET /projects/:id/merge_requests/:iid` |
| Дифф файлов             | `GET /projects/:id/merge_requests/:iid/diffs` (пагинация) |
| Полная версия файла     | `GET /projects/:id/repository/files/:path/raw?ref=<sha>` |
| Инлайн-комментарий      | `POST /projects/:id/merge_requests/:iid/discussions` с `position` |

`:id` — URL-encoded путь проекта из URL MR.
TLS — `rustls` с системными сертификатами (корпоративные CA из системного
хранилища подхватываются).

## Последствия

- Скорость зависит от сети; полные файлы для режима Page грузим лениво.
  Если окажется медленно — рассмотрим локальный `git fetch` MR-ветки.
- `position` требует `base_sha`/`start_sha`/`head_sha` из `diff_refs`
  и `old_line`/`new_line` — модель `LineRef` спроектирована под это.

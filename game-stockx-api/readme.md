# GameStockX API

## Структура

- `auth.rs` — вход, JWT и общие функции чтения Bearer-токена.
- `collection/models.rs` — форматы запросов и ответов коллекции, wishlist и WTS.
- `collection/read.rs` — чтение списков и статистики.
- `collection/mutations.rs` — добавление, удаление и изменение цены.
- `collectors.rs` — список коллекционеров и WTS игрока.
- `product_list.rs`, `product_details.rs` — каталог и карточка игры.
- `redis.rs` — существующий пул Redis и сериализация кэша.
- `chat.rs`, `metrics_middleware.rs`, `simple_rate_limiter.rs` — чат и middleware.

Рефакторинг сохраняет SQL-запросы, маршруты, форматы ответов, тексты ошибок,
проверки доступа, TTL/ключи кэша и существующие значения метрик.
Миграции и скрипт запуска в рамках рефакторинга не изменяются.

## Проверки

```bash
cargo fmt --check
cargo clippy --locked --offline --all-targets -- -D warnings
cargo test --locked --offline
cargo build --locked --offline
```

Для `--offline` зависимости должны быть установлены заранее.

`tests/http_contract.py` сравнивает два заранее собранных бинарника API.
Нужны Docker, образы `postgres:15` и `redis:alpine`, Diesel CLI в PATH.
Скрипт создаёт отдельные временные контейнеры, применяет миграции, заполняет
тестовые данные и сравнивает 70 HTTP-ответов (статус, Content-Type и содержимое).
Рабочие базы и контейнеры не используются. Тестовые контейнеры удаляются после проверки.

```bash
python3 tests/http_contract.py /absolute/path/before-api /absolute/path/after-api
```

Проверяются авторизация, каталог и кэш, 404 карточки, коллекция/wishlist/WTS,
цены, CIB, удаление отметок и чтение чата. Тест не заменяет проверку WebSocket
и нагрузочное тестирование.

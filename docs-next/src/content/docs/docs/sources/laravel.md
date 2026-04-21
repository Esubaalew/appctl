---
title: Laravel API
description: Read routes/api.php and database/migrations/* to produce REST tools for a Laravel API.
---

Reads `routes/api.php` and files in `database/migrations/` to generate REST tools for a Laravel API project.

## Prerequisites

- A Laravel 10 or 11 project folder.
- To run the demo server: PHP 8.2 or newer and Composer.
- `appctl` installed.

## The demo in this repo

[`examples/demos/laravel-api/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/laravel-api) is a minimal Laravel 11 app with `Post` and `Comment` resources registered through `Route::apiResource`. The routes file and migrations are committed, so the sync runs without PHP installed.

### Sync without running PHP

```bash
cd examples/demos/laravel-api
appctl sync --laravel . --base-url http://127.0.0.1:8002 --force
```

Real output:

```
Synced Laravel: 2 resources, 10 tools written to .appctl
```

Generated tools:

```
post:    posts_list GET /api/posts, post_get GET /api/posts/{id},
         post_create POST /api/posts, post_update PATCH /api/posts/{id},
         post_delete DELETE /api/posts/{id}
comment: same five tools under /api/comments
```

### Run the live server (optional)

PHP 8.2 or newer and Composer are required. Install Composer from [getcomposer.org](https://getcomposer.org/) if your system does not have it.

```bash
php -v           # 8.2 or higher
composer -V      # should exist

cd examples/demos/laravel-api
composer install
cp .env.example .env  # if present, or create .env
php artisan key:generate
touch database/database.sqlite
php artisan migrate
php artisan serve --host=127.0.0.1 --port=8002
```

Check:

```bash
curl -s -X POST http://127.0.0.1:8002/api/posts \
  -H "Content-Type: application/json" \
  -d '{"title":"hi","body":"hello"}'
```

The static sync is independent of the live server, so the tool list above is correct even if Composer is not installed on your box.

## What appctl reads

- `routes/api.php`: only `Route::apiResource('foos', FooController::class)` lines are parsed. Manually defined routes are ignored.
- `database/migrations/*.php`: `Schema::create` blocks for column names and types.

## Known limits

- Middleware on routes (`->middleware('auth:sanctum')`) is not represented in the generated tools.
- Form Requests and custom validation rules are not extracted. Tool parameter schemas come from migration columns only.
- If you already publish a Swagger document with `l5-swagger`, use [`--openapi`](/docs/sources/openapi/).

## See also

- [`appctl sync`](/docs/cli/sync/)
- [OpenAPI source](/docs/sources/openapi/)

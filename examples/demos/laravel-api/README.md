# laravel-api demo

A minimal Laravel 11 API with two resources: `Post` and `Comment`.
This is the app `appctl sync --laravel` reads.

## What's here

```
composer.json
routes/api.php                 Route::apiResource for posts and comments
database/migrations/           creates posts + comments tables
app/Models/                    Post, Comment (Eloquent)
app/Http/Controllers/          PostController, CommentController (full CRUD)
```

## Quick start

Requires PHP 8.2+ and Composer.

```sh
# 1. Install deps, migrate, start server on :8002
make up

# 2. Sync appctl
make sync

# 3. Ask something
make chat MSG="list all posts"
```

## What appctl syncs

`appctl sync --laravel .` reads:
- `routes/api.php` to discover `apiResource` declarations
- `database/migrations/` for table columns and types

It generates HTTP tools for `posts` and `comments`. Fields come from the migration column definitions.

## Known limits

- `apiResource` routes are parsed; manually defined `Route::get(...)` entries are not.
- Polymorphic relations are not expanded.
- No authentication in this demo. Pass `--auth-header "Bearer yourtoken"` when syncing a protected app.

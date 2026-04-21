# Demo: Laravel API

```bash
composer create-project laravel/laravel demo
cd demo && php artisan install:api
php artisan serve --host=127.0.0.1 --port=8000
```

```bash
appctl sync --laravel ./demo --base-url http://127.0.0.1:8000 --force
```

## Makefile

`make sync LARAVEL_ROOT=./demo`

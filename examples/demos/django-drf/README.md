# django-drf demo

A minimal Django 4.2 + Django REST Framework API with two models (`Parcel`
and `Customer`). This is the app `appctl sync --django` reads.

## What's here

```
manage.py
requirements.txt
project/
  settings.py   uses SQLite, no env vars required
  urls.py       mounts billing.urls at /api/
billing/
  models.py     Parcel and Customer
  serializers.py
  views.py      ModelViewSet for each model
  urls.py       DefaultRouter
```

## Quick start (copy, paste, wait a few seconds)

```sh
# 1. Install deps, create the SQLite db, run the server on :8001
make up

# 2. In another terminal, sync appctl
make sync

# 3. Ask a question (needs an LLM configured in .appctl/config.toml)
make chat MSG="list all parcels"
```

## What appctl reads

`appctl sync --django .` looks at:

- `billing/models.py` to discover the `Parcel` and `Customer` models.
- `project/settings.py` to confirm `rest_framework` is installed.

It does **not** parse `project/urls.py` for the `/api/` prefix. You have to
pass it yourself in `--base-url`:

```
appctl sync --django . --base-url http://127.0.0.1:8001/api --force
```

Real output on a clean machine:

```
Synced Django: 2 resources, 10 tools written to .appctl
```

Five tools per model: list, get, create, update, delete.

## Gotchas

- **First run needs `makemigrations`.** The initial migration for the `billing`
  app is generated from your models, so `make migrate` runs both
  `makemigrations billing` and `migrate`.
- **`--base-url` must include `/api`.** Without it, the generated tool paths
  will 404.
- **`appctl doctor` requires `django_api_token`.** Even though this demo uses
  `AllowAny` permissions, the default auth strategy for Django sync is a
  Bearer token. Set any value (for example `django_api_token=unused`) before
  running doctor.
## Tear down

Stop the runserver process with Ctrl+C, or:

```
make down
```

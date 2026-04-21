---
title: Django (DRF)
description: Introspect a Django + DRF project and get five REST tools per model.
---

Point `appctl` at a Django project that uses Django REST Framework. It reads your models and generates five tools per model.

## Prerequisites

- A Django project folder. The parser looks for `manage.py` at the top and `models.py` inside each app.
- `rest_framework` in your `INSTALLED_APPS`. Without it the parser still runs, but the generated routes will not match anything real.
- Python 3.11 or newer to run the demo.
- `appctl` installed.

## The demo in this repo

[`examples/demos/django-drf/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/django-drf) is a Django 4.2 project with a `billing` app that has two models (`Parcel` and `Customer`). A `DefaultRouter` serves them at `/api/parcels/` and `/api/customers/`.

### 1. Set up Python

```bash
cd examples/demos/django-drf
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

### 2. Create the database

The first run needs `makemigrations` because the initial migration for the `billing` app is generated from your models, not committed:

```bash
python manage.py makemigrations billing
python manage.py migrate
```

Output ends with:

```
Applying billing.0001_initial... OK
```

### 3. Start the server

```bash
python manage.py runserver 127.0.0.1:8001
```

Sanity check:

```bash
curl -s -X POST http://127.0.0.1:8001/api/parcels/ \
    -H "Content-Type: application/json" \
    -d '{"tracking_number":"PK-001","weight_kg":"1.5","delivered":false}'
```

Output:

```
{"id":1,"tracking_number":"PK-001","weight_kg":"1.50","delivered":false}
```

### 4. Sync appctl

Pass the path to the Django project folder, and a `--base-url` that includes your API mount prefix (`/api` in this demo):

```bash
appctl sync --django . --base-url http://127.0.0.1:8001/api --force
```

Real output with `appctl 0.2.0`:

```
Synced Django: 2 resources, 10 tools written to .appctl
```

Generated tools:

```
parcel:   list_parcels, get_parcel, create_parcel, update_parcel, delete_parcel
customer: list_customers, get_customer, create_customer, update_customer, delete_customer
```

### 5. Talk to it

```bash
appctl chat "how many parcels have been delivered"
appctl chat "create a parcel with tracking PK-002 weighing 2kg"
```

## What appctl reads

- `manage.py` to confirm this is a Django project.
- Any `settings.py` file, to pick up `INSTALLED_APPS` and the root URL conf.
- `*/models.py` for each installed app, to extract model names and field types.

It does not execute your Python code. If your models use dynamic class creation or a custom `ModelMeta` base, the parser will miss them.

## Troubleshooting

- **Pass `/api` in `--base-url`.** The sync writes tool paths as `/parcels/`, not `/api/parcels/`. If you pass the raw `http://127.0.0.1:8001` the tools will hit the wrong URL. Always include the prefix.
- **`appctl doctor` needs `django_api_token` set.** Even though this demo uses `AllowAny`, the default Django auth strategy is a Bearer token. Export `django_api_token=unused` before running doctor.
- **Model migrations.** The first sync after cloning needs `makemigrations billing && migrate` before the API returns anything.

## Known limits

- Function based DRF views are not parsed. Only `ModelViewSet` registered on a `DefaultRouter`.
- Custom `@action` methods on a ViewSet are not extracted.
- If your project publishes a Swagger document with `drf-spectacular`, use [`--openapi`](/docs/sources/openapi/); the output is better.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [OpenAPI source](/docs/sources/openapi/)

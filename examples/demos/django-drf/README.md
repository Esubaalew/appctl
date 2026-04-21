# Demo: Django + DRF

Minimal pattern for `appctl sync --django` against a project that includes **Django REST Framework** in `INSTALLED_APPS`.

## Quick path (local project)

```bash
django-admin startproject demo && cd demo
python -m pip install django djangorestframework
# Add `rest_framework` to INSTALLED_APPS; add a model + ModelViewSet + router URLs.
appctl sync --django . --base-url http://127.0.0.1:8000 --auth-header "Bearer YOUR_TOKEN"
appctl doctor --write
appctl serve --token "$(openssl rand -hex 12)"
```

## Reference in this repo

CI exercises Django sync using `crates/appctl/tests/fixtures/django_app` (see `e2e_django_drf` test).

## Makefile

```bash
make sync   # after you have a running server + project path in DJANGO_ROOT
```

See `Makefile` in this directory.

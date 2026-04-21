# url-login demo

A tiny Flask app with a session-based login form.
This shows `appctl sync --url` authenticating via a web form before scraping routes.

## What's here

```
app.py             Flask: GET /login, POST /login (sets session), GET /, GET /logout
requirements.txt   flask only
```

## Quick start

Requires Python 3.11+.

```sh
# 1. Create venv, install deps, start Flask on :5009
make up

# 2. In another terminal: sync appctl using the login URL
make sync

# 3. Ask something
make chat MSG="what pages are accessible"
```

## What appctl does

`appctl sync --url` starts a headless browser, navigates to the login URL, fills the
form fields with the supplied credentials, then follows redirects to discover
authenticated routes.

```sh
appctl sync --url http://127.0.0.1:5009 \
  --login-url http://127.0.0.1:5009/login \
  --login-user you \
  --login-password secret \
  --force
```

The default credentials accepted by this demo are any non-empty username and any password
(the app stores whatever you type as the session user).

## Known limits

- `appctl sync --url` discovers routes by following links in the rendered HTML; it does
  not execute JavaScript single-page apps.
- This demo has no API endpoints. It is useful for form-based login flow testing, not for
  generating JSON API tools.

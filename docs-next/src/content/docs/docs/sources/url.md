---
title: URL login (HTML)
description: For old-school websites. Log in through a form, keep the cookie, and discover forms and links.
---

For old-school websites without a public API. `appctl` logs in through a form, stores the session cookie, and discovers any forms and links it can reach.

## Prerequisites

- A login URL that accepts username and password as form fields.
- A session cookie flow after login. Pure JavaScript single-page apps are not in scope; use their backing API with [`--openapi`](/docs/sources/openapi/) instead.
- `appctl` installed.

## The demo in this repo

[`examples/demos/url-login/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/url-login) is a short Flask app with one login form (`/login`) and one protected page (`/`).

### 1. Start the server

```bash
cd examples/demos/url-login
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python app.py
```

Sanity check:

```bash
curl -s -c /tmp/cookie -X POST http://127.0.0.1:5009/login \
    -d "user=alice&password=secret" -o /dev/null \
    -w "status=%{http_code}\n"
# status=302

curl -s -b /tmp/cookie http://127.0.0.1:5009/
# <p>ok alice</p><a href=/logout>logout</a>
```

### 2. Run the sync

```bash
appctl sync --url http://127.0.0.1:5009/ \
  --login-url http://127.0.0.1:5009/login \
  --login-user alice --login-password secret --force
```

Real output:

```
INFO logging in at http://127.0.0.1:5009/login as alice
Synced Url: 0 resources, 0 tools written to .appctl
```

The login worked, but the post-login page here has no discoverable structure beyond a logout link. For the crawler to produce tools, the post-login page needs forms, tables, or links with an action attribute it can interpret.

## What appctl does in this mode

- Issues a `POST` to the login URL with `user` and `password` fields by default. Change the field names with `--login-user-field` and `--login-password-field`.
- Stores cookies in memory for the duration of the sync.
- Starts from the base URL and follows links up to a small depth, turning each form it finds into a tool.

## Known limits

- Sites that sign requests (CSRF tokens, CAPTCHAs, two-factor) will not work with this source. Use a real integration.
- Single-page apps that fetch JSON over XHR after load are invisible to a static crawler. Intercept the XHR calls and feed the resulting API to [`--openapi`](/docs/sources/openapi/).
- Session cookies are not rotated during a chat session. Long-running `appctl serve` deployments may need to re-login periodically.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [OpenAPI source](/docs/sources/openapi/)

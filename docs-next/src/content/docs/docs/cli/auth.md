---
title: appctl auth
description: Run OAuth device or authorization-code flows for an app you are syncing.
---

Run OAuth flows for apps that require a user token at call time (GitHub API, Slack, custom internal services).

## Usage

```
appctl auth <COMMAND>
```

Commands:

- `login <provider>` — run an OAuth authorization-code flow with a local redirect.
- `status` — show stored tokens and their expiry.

## `appctl auth login`

```
appctl auth login [OPTIONS] <PROVIDER>
```

### Options

- `--client-id <ID>`
- `--client-secret <SECRET>`
- `--auth-url <URL>` — authorization endpoint.
- `--token-url <URL>` — token exchange endpoint.
- `--scope <SCOPE>` — OAuth scope (provider-specific).
- `--redirect-port <PORT>` — local port for the redirect listener (default `8421`).

### Example (GitHub)

```bash
appctl auth login github \
  --client-id "$GH_CLIENT_ID" \
  --client-secret "$GH_CLIENT_SECRET" \
  --auth-url https://github.com/login/oauth/authorize \
  --token-url https://github.com/login/oauth/access_token \
  --scope "repo read:user"
```

`appctl` opens the browser, waits for the callback on `localhost:<redirect-port>`, exchanges the code for a token, and stores it in the keychain as `oauth:<provider>`.

## `appctl auth status`

```bash
appctl auth status
```

Output:

```
provider  stored  expires_in
github    yes     6d 22h
```

Tokens refresh automatically when a refresh token is present.

## Using the token in sync

Reference the provider in your schema's `auth` block:

```json
"auth": { "kind": "oauth_flow", "provider": "github" }
```

`appctl` fetches the token at call time and sets `Authorization: Bearer <token>` on every HTTP tool.

## Related

- [Secrets and keys](/docs/deploy/secrets-and-keys/)
- [Security](/docs/security/)

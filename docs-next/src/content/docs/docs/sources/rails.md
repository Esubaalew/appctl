---
title: Rails API
description: Read config/routes.rb and db/schema.rb to produce REST tools for a Rails API app.
---

Reads `config/routes.rb` and `db/schema.rb` to generate tools for a Rails API-only app. Controllers are not executed; everything works from static files.

## Prerequisites

- A Rails 7 project folder (`config/application.rb`, `config/routes.rb`, `db/schema.rb` present).
- To run the demo server: Ruby 3.1 or newer and Bundler 2.
- `appctl` installed.

## The demo in this repo

[`examples/demos/rails-api/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/rails-api) is an API-only Rails 7.1 app with `Post` and `Comment` resources mounted under `/api/v1/`. The static files that `appctl` reads are committed. You can run the sync without installing Ruby.

### Sync without running Ruby

```bash
cd examples/demos/rails-api
appctl sync --rails . --base-url http://127.0.0.1:3001 --force
```

Real output:

```
Synced Rails: 2 resources, 10 tools written to .appctl
```

Generated tools:

```
post:    posts_list GET /api/v1/posts, post_get GET /api/v1/posts/{id},
         post_create POST /api/v1/posts, post_update PATCH /api/v1/posts/{id},
         post_delete DELETE /api/v1/posts/{id}
comment: same five tools under /api/v1/comments
```

If you see singular paths (`/api/v1/post`) you are on an older `appctl`. Upgrade past 0.2.0.

### Run the live server (optional)

Ruby 3.1 or newer is required. Use `rbenv`, `asdf`, or `rvm` if your system Ruby is older.

```bash
ruby -v          # should print 3.1 or higher
bundle install
bundle exec rake db:create db:migrate
bundle exec rails server -p 3001
```

Verify:

```bash
curl -s -X POST http://127.0.0.1:3001/api/v1/posts \
  -H "Content-Type: application/json" \
  -d '{"post":{"title":"hi","body":"hello"}}'
```

## What appctl reads

- `config/routes.rb`: only `resources :foo` lines inside `namespace` blocks are parsed.
- `db/schema.rb`: `create_table "foos"` blocks, for field names and types.

Controllers are not read. Strong parameters are not inspected. Custom routes (`get "/foo/bar"`) are ignored.

## Known limits

- Namespaced routes deeper than one level are not parsed.
- Custom serializers and jbuilder views do not affect the generated tools.
- No `has_many` / `belongs_to` relation awareness yet.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [OpenAPI source](/docs/sources/openapi/)

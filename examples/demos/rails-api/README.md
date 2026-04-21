# rails-api demo

A minimal Rails 7 API-only app with two resources: `Post` and `Comment`.
This is the app `appctl sync --rails` reads.

## What's here

```
Gemfile              rails ~7.1, puma, rack-cors
config/routes.rb     namespace :api/:v1 with resources :posts and :comments
db/schema.rb         posts + comments tables
app/models/          Post, Comment
app/controllers/api/v1/
  posts_controller.rb     full CRUD
  comments_controller.rb  index, show, create
config/database.yml  SQLite (no extra deps)
```

## Quick start

Requires Ruby 3.x and Bundler.

```sh
# 1. Install gems, create DB, load schema, start server on :3001
make up

# 2. In another terminal: sync appctl
make sync

# 3. Ask something
make chat MSG="create a post titled Hello"
```

## What appctl syncs

`appctl sync --rails .` reads `db/schema.rb` and `config/routes.rb`.

It sees `resources :posts` and generates 5 HTTP tools (list, get, create, update, delete)
at `/api/v1/posts`. It sees `resources :comments, only: [...]` and generates 3 tools.

Fields come from `schema.rb` column definitions.

## Known limits

- No authentication in this demo. Add `--auth-header "Bearer yourtoken"` to the sync
  command if you lock the API down.
- `appctl sync --rails` does not execute the app, so routes that Rails resolves at
  runtime (e.g. via concerns or dynamic constraints) are not discovered.

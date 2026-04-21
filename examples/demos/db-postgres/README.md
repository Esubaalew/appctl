# Demo: PostgreSQL

Seeded schema for `appctl sync --db`.

## Commands

```bash
make up
export DATABASE_URL=postgres://appctl:appctl@127.0.0.1:5433/appctl_demo
make sync
appctl chat "list widgets"   # requires LLM config
make down
```

## Env

See `.env.example`.

# Demo: Supabase

Use your Supabase project REST URL and anon key:

```bash
appctl sync --supabase https://YOUR_REF.supabase.co --supabase-anon-ref SUPABASE_ANON_KEY --force
```

Store the anon key with `appctl config set-secret SUPABASE_ANON_KEY` (or env var).

## Makefile

`make sync` uses `SUPABASE_URL` from the environment (see `.env.example`).

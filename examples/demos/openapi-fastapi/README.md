# Demo: OpenAPI (FastAPI)

One-file API with auto-generated OpenAPI — the canonical `appctl sync --openapi` path.

## Prerequisites

- Python 3.11+
- `appctl` built or `cargo install appctl`

## Commands

```bash
make up        # install deps + run uvicorn on :8000
make sync      # appctl sync against live OpenAPI
make chat MSG='create a widget named Demo'   # requires LLM config
make down      # stop server (if backgrounded)
```

Or manually:

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
uvicorn main:app --host 127.0.0.1 --port 8000
```

In another terminal:

```bash
appctl sync --openapi http://127.0.0.1:8000/openapi.json --base-url http://127.0.0.1:8000
appctl doctor
appctl chat "create a widget named Demo"
```

## Files

- `openapi.json` — static spec used by CI e2e tests (matches `main.py` routes).
- `main.py` — runnable reference server.

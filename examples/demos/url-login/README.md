# Demo: URL login (Flask)

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
python app.py
```

```bash
appctl sync --url http://127.0.0.1:5009/login --login-url http://127.0.0.1:5009/login \
  --login-user you --login-password secret --force
```

Adjust flags to match your form fields (see `appctl sync --help`).

## Makefile

`make up` runs Flask on port 5009.

PY ?= python3
PORT ?= 8000
OPENAPI ?= http://127.0.0.1:$(PORT)/openapi.json

.PHONY: up sync chat down venv

venv:
	$(PY) -m venv .venv
	./.venv/bin/pip install -r requirements.txt

up: venv
	./.venv/bin/uvicorn main:app --host 127.0.0.1 --port $(PORT)

sync:
	appctl sync --openapi $(OPENAPI) --base-url http://127.0.0.1:$(PORT) --force

chat:
	appctl chat "$(MSG)"

down:
	@echo "Stop the uvicorn process you started (Ctrl+C or kill)."

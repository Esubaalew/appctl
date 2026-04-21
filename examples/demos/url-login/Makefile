PY ?= python3
PORT ?= 5009
BASE ?= http://127.0.0.1:$(PORT)

.PHONY: venv up sync chat down

venv:
	$(PY) -m venv .venv
	./.venv/bin/pip install -r requirements.txt

up: venv
	./.venv/bin/python app.py

sync:
	appctl sync --url $(BASE) \
	  --login-url $(BASE)/login \
	  --login-user you \
	  --login-password secret \
	  --force

chat:
	appctl chat "$(MSG)"

down:
	@echo "Stop the Flask process (Ctrl+C or kill)."

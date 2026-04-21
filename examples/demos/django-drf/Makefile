PY ?= python3
PORT ?= 8001
BASE ?= http://127.0.0.1:$(PORT)

.PHONY: venv migrate up sync chat down

venv:
	$(PY) -m venv .venv
	./.venv/bin/pip install -r requirements.txt

migrate: venv
	DJANGO_SETTINGS_MODULE=project.settings ./.venv/bin/python manage.py makemigrations billing
	DJANGO_SETTINGS_MODULE=project.settings ./.venv/bin/python manage.py migrate

up: migrate
	DJANGO_SETTINGS_MODULE=project.settings ./.venv/bin/python manage.py runserver 127.0.0.1:$(PORT)

sync:
	appctl sync --django . --base-url $(BASE)/api --force

chat:
	appctl chat "$(MSG)"

down:
	@echo "Stop the runserver process (Ctrl+C or kill)."

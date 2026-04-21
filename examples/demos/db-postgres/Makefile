DATABASE_URL ?= postgres://appctl:appctl@127.0.0.1:5433/appctl_demo

.PHONY: up down sync chat
up:
	docker compose up -d

down:
	docker compose down

sync:
	appctl sync --db "$(DATABASE_URL)" --force

chat:
	appctl chat "$(MSG)"

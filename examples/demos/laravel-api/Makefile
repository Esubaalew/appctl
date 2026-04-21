PORT ?= 8002
BASE ?= http://127.0.0.1:$(PORT)

.PHONY: setup db up sync chat down

setup:
	composer install

db: setup
	php artisan migrate --force

up: db
	php artisan serve --host=127.0.0.1 --port=$(PORT)

sync:
	appctl sync --laravel . --base-url $(BASE) --force

chat:
	appctl chat "$(MSG)"

down:
	@echo "Stop the artisan server (Ctrl+C or kill)."

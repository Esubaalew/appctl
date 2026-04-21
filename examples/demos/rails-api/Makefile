PORT ?= 3001
BASE ?= http://127.0.0.1:$(PORT)

.PHONY: setup db up sync chat down

setup:
	bundle install

db: setup
	bundle exec rails db:create db:schema:load

up: db
	bundle exec rails server -p $(PORT) -b 127.0.0.1

sync:
	appctl sync --rails . --base-url $(BASE) --force

chat:
	appctl chat "$(MSG)"

down:
	@echo "Stop the rails server (Ctrl+C or kill)."

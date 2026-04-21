PORT ?= 1337
BASE ?= http://localhost:$(PORT)

.PHONY: sync

sync:
	appctl sync --strapi . --base-url $(BASE) --force

chat:
	appctl chat "$(MSG)"

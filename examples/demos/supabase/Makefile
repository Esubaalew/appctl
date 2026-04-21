POSTGREST_URL ?= http://localhost:3010
ANON_REF ?= SUPABASE_ANON_KEY

.PHONY: up sync chat down

up:
	docker compose up -d
	@echo "PostgREST ready at $(POSTGREST_URL)"

sync:
	appctl sync --supabase $(POSTGREST_URL) --supabase-anon-ref $(ANON_REF) --force

chat:
	appctl chat "$(MSG)"

down:
	docker compose down -v

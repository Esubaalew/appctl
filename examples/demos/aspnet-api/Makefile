PORT ?= 5001
BASE ?= http://localhost:$(PORT)

.PHONY: build up sync chat down

build:
	dotnet build

up: build
	dotnet run --urls "http://localhost:$(PORT)"

sync:
	appctl sync --aspnet . --base-url $(BASE) --force

chat:
	appctl chat "$(MSG)"

down:
	@echo "Stop the dotnet process (Ctrl+C or kill)."

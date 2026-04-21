# Usage: make sync DJANGO_ROOT=/path/to/django/project
DJANGO_ROOT ?= .

.PHONY: sync
sync:
	appctl sync --django $(DJANGO_ROOT) --force

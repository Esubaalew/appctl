# Demo: Rails API

```bash
rails new demo --api --minimal
cd demo && rails g scaffold Item name:string && rails db:migrate
rails s -p 3000
```

In another shell:

```bash
appctl sync --rails ./demo --base-url http://127.0.0.1:3000 --force
appctl doctor --write
```

## Makefile

`make sync RAILS_ROOT=./demo` runs `appctl sync --rails`.

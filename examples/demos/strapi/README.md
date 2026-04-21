# Demo: Strapi

```bash
npx create-strapi-app@latest demo --quickstart
cd demo && npm run develop
```

Then point `appctl sync --strapi` at the generated project directory and set `--base-url` to your public URL.

## Makefile

`make sync STRAPI_ROOT=./demo`

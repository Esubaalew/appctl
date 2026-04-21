# strapi demo

Two Strapi v4 content-types committed as schema JSON: `Article` and `Product`.
This is what `appctl sync --strapi` reads.

## What's here

```
src/api/
  article/content-types/article/schema.json   Article (draft + publish, 4 fields)
  product/content-types/product/schema.json   Product (3 fields)
```

## How appctl syncs

`appctl sync --strapi .` reads every `schema.json` under `src/api/` and generates
5 HTTP tools per content-type (list, get, create, update, delete) using the standard
Strapi REST routes (`/api/articles`, `/api/products`).

```sh
make sync
```

To point at a live Strapi instance, pass `--base-url`:

```sh
appctl sync --strapi . --base-url http://localhost:1337 --force
```

Then set the API token and chat:

```sh
export STRAPI_API_TOKEN=your-full-access-token
appctl chat "list all articles"
```

## Running a live Strapi server

To test against a real Strapi instance, run a local development server:

```sh
npx create-strapi-app@latest my-strapi --quickstart
```

Copy the `src/api/` folder from this demo into the generated project, restart the server,
generate an API token in the Strapi admin, and you have a live backend for full end-to-end testing.

## Known limits

- `appctl sync --strapi` reads only `src/api/<name>/content-types/<name>/schema.json`.
  Nested components and dynamic zones are not expanded.
- The `publishedAt` pattern (draft/publish) is treated as a plain datetime field.

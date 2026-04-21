---
title: Strapi
description: Turn Strapi v4 content types into REST tools.
---

Reads `src/api/*/content-types/*/schema.json` files from a Strapi v4 project and turns each content type into a REST resource with five tools.

## Prerequisites

- A Strapi v4 project folder. Strapi v3 uses a different schema format and is not supported.
- `appctl` installed.
- Running the live Strapi server is optional for sync. It is only needed to actually call the generated tools.

## The demo in this repo

[`examples/demos/strapi/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/strapi) contains two committed schemas:

- `src/api/article/content-types/article/schema.json`, fields `title`, `body`, `publishedAt`, `featured`.
- `src/api/product/content-types/product/schema.json`, fields `name`, `price`, `inStock`.

### Sync

```bash
cd examples/demos/strapi
appctl sync --strapi . --base-url http://localhost:1337 --force
```

Real output:

```
Synced Strapi: 2 resources, 10 tools written to .appctl
```

Generated tools:

```
product: product_list GET /api/products, product_get GET /api/products/{id},
         product_create POST /api/products, product_update PUT /api/products/{id},
         product_delete DELETE /api/products/{id}
article: same five tools under /api/articles
```

### Run Strapi (optional)

Starting a Strapi v4 server needs Node.js 18 or newer and takes a while on first boot.

```bash
npx create-strapi-app@latest my-strapi --quickstart
# copy examples/demos/strapi/src/api/* into my-strapi/src/api/
cd my-strapi && npm run develop
```

Then run the sync against the new project folder and use its URL.

## What appctl reads

- Every `schema.json` under `src/api/*/content-types/*/`.
- The `singularName`, `pluralName`, and `attributes` sections of each schema.
- For each content type, five standard REST tools are generated against `/api/{pluralName}`.

## Known limits

- Strapi permissions (public, authenticated, custom roles) are not visible at sync time. You will see 403s at call time if the anon role lacks the action.
- Nested components and relations are flattened to string types in the tool schemas.
- Only the standard `/api/*` REST endpoints are generated. Custom controllers or GraphQL resolvers are not.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [OpenAPI source](/docs/sources/openapi/)

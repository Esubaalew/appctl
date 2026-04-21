---
title: ASP.NET Core
description: Prefer swagger.json; fall back to scanning [ApiController] classes.
---

Two paths. If the project exports a `swagger.json`, it is used. Otherwise controllers annotated with `[ApiController]` are scanned.

## Prerequisites

- An ASP.NET Core project folder with a `.csproj`.
- The .NET SDK (8, 9, or 10) to build the demo.
- `appctl` installed.

## The demo in this repo

[`examples/demos/aspnet-api/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/aspnet-api) is a minimal Web API project with an `ItemsController` that exposes full CRUD over an in-memory list. It includes `Swashbuckle.AspNetCore` so Swagger is available at `/swagger/v1/swagger.json`.

### 1. Build and run

```bash
cd examples/demos/aspnet-api
dotnet build
dotnet run --urls "http://localhost:5001"
```

Real build output on this machine (dotnet 10.0.100):

```
DemoApi -> bin/Debug/net8.0/DemoApi.dll
Build succeeded. 0 Warning(s) 0 Error(s)
```

Check a real endpoint:

```bash
curl -s -X POST http://localhost:5001/api/Items \
    -H "Content-Type: application/json" \
    -d '{"name":"NB","description":"Notebook"}'
```

Output:

```
{"id":1,"name":"NB","description":"Notebook"}
```

### 2. Sync path A: `swagger.json` (preferred)

With the server running, save the Swagger document and run sync. `appctl` detects `swagger.json` in the project folder and delegates to OpenAPI sync.

```bash
curl -s http://localhost:5001/swagger/v1/swagger.json -o swagger.json
appctl sync --aspnet . --base-url http://localhost:5001 --force
```

Real output:

```
delegating to OpenAPI sync via ./swagger.json
Synced Aspnet: 1 resources, 5 tools written to .appctl
```

Generated tools:

```
items: list_items, create_items, get_items, update_items, delete_items
```

### 3. Sync path B: controller scan (no `swagger.json`)

```bash
rm swagger.json
appctl sync --aspnet . --base-url http://localhost:5001 --force
```

Real output:

```
Synced Aspnet: 1 resources, 5 tools written to .appctl
```

Generated tools (naming is rougher):

```
Items: items_ok, items_getbyid, items_create, items_update, items_delete
```

The first tool is called `items_ok` because the controller scan reads the C# identifier near the `[HttpGet]` attribute, which is the `Ok(...)` method call, not the C# method name. Prefer the Swagger path when you can.

## What appctl reads

- `swagger.json` if present anywhere in the project directory. When found, the sync defers to OpenAPI.
- Otherwise, `.cs` files with a `[Route(...)]` attribute and any of `[HttpGet]`, `[HttpPost]`, `[HttpPatch]`, `[HttpDelete]`.

## Tips

- Keep `Swashbuckle` and export swagger at build time. You get cleaner tool names and correct parameter schemas.
- The controller scan does not read `[FromBody]` model types; parameter schemas will be empty.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [OpenAPI source](/docs/sources/openapi/)

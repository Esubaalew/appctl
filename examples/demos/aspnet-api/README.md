# aspnet-api demo

A minimal ASP.NET Core 8 Web API with a single `Items` resource stored in memory.
This is the app `appctl sync --aspnet` reads.

## What's here

```
DemoApi.csproj             targets net8.0, includes Swashbuckle
Program.cs                 adds controllers, Swagger, runs the app
Controllers/
  ItemsController.cs       [ApiController] with GET/POST/PATCH/DELETE
```

## Quick start

Requires .NET 8 SDK.

```sh
# 1. Build and start the server on :5001
make up

# 2. Swagger UI is available at http://localhost:5001/swagger
# 3. Sync appctl
make sync

# 4. Ask something
make chat MSG="create an item named Notebook"
```

## Two sync paths

`appctl sync --aspnet .` tries two things in order:

1. If a `swagger.json` or `openapi.json` exists anywhere under the project root, it reads
   that file for accurate type information (best option).
2. Otherwise it scans `*.cs` files for `[ApiController]` classes and `[HttpGet/Post/...]`
   attributes (heuristic; works for this demo).

To get the more accurate path, first run the server and export the schema:

```sh
curl http://localhost:5001/swagger/v1/swagger.json -o swagger.json
appctl sync --aspnet . --base-url http://localhost:5001 --force
```

## Known limits

- In-memory store; data resets on restart.
- Controller scanning does not parse `[FromBody]` field names; use the swagger.json path for full field coverage.

# Demo: ASP.NET Web API

```bash
dotnet new webapi -o DemoApi
cd DemoApi && dotnet run --urls http://127.0.0.1:5088
```

```bash
appctl sync --aspnet ./DemoApi --base-url http://127.0.0.1:5088 --force
```

## Makefile

`make sync ASPNET_ROOT=./DemoApi`

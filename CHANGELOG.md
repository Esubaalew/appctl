# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial public release of the `appctl` workspace.
- `appctl` CLI with commands: `sync`, `chat`, `run`, `history`, `serve`, `config`, `plugin`, `auth`.
- `appctl-plugin-sdk` crate exposing a stable schema plus a C ABI so third-party
  plugins can be shipped as dynamic libraries.
- Built-in sync plugins: `openapi`, `django`, `db`, `url` (with login + CSRF +
  cookie jar), `mcp`, `rails`, `laravel`, `aspnet`, `strapi`, `supabase`.
- Dynamic plugin discovery under `~/.appctl/plugins/` and an example
  `appctl-airtable` plugin in `examples/plugins/`.
- OAuth2 PKCE login (`appctl auth login`) with keychain-persisted refresh
  tokens; executor injects bearer tokens for `AuthStrategy::OAuth2` resources.
- `appctl serve` HTTP + WebSocket daemon plus a VS Code extension in
  `extensions/vscode/` that renders chat and tool traces.
- Provider-agnostic LLM layer with OpenAI-compatible + NVIDIA/OpenRouter
  presets and a configurable agent loop.

## [0.1.0] - TBD

_First tagged release._

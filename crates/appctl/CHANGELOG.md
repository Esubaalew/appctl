# Changelog

All notable changes to `appctl` are documented in this file.

## [Unreleased]

## [0.12.1](https://github.com/Esubaalew/appctl/compare/appctl-v0.12.0...appctl-v0.12.1) - 2026-04-28

### Fixed

- fix; some issue

## [0.12.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.11.0...appctl-v0.12.0) - 2026-04-27

### Added

- *(setup)* make appctl easiy to setup

## [0.11.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.10.0...appctl-v0.11.0) - 2026-04-27

### Fixed

- make app flow good

## [0.10.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.9.1...appctl-v0.10.0) - 2026-04-27

### Fixed

- make app flow good

## [0.9.1](https://github.com/Esubaalew/appctl/compare/appctl-v0.9.0...appctl-v0.9.1) - 2026-04-24

### Other

- smarter openai ger and docs

## [0.9.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.8.0...appctl-v0.9.0) - 2026-04-24

## [0.8.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.7.1...appctl-v0.8.0) - 2026-04-24

### Added

- OpenAPI sync: use `--auth-header` when **fetching** the document over HTTP(S); support `env:VAR` / `Bearer env:VAR` in that header; send `User-Agent` and `Accept` for JSON/YAML; if the URL is a **site root** and the first GET is 404, try common paths (`/openapi.json`, `/v3/api-docs`, …).

## [0.7.1](https://github.com/Esubaalew/appctl/compare/appctl-v0.7.0...appctl-v0.7.1) - 2026-04-24

### Fixed

- doc app drift
- `appctl sync` refuses to replace an existing `schema.json` without `--force`, matching the documented safety behavior.

## [0.7.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.6.0...appctl-v0.7.0) - 2026-04-23

### Added

- add watch sync, named sessions, runtime tool policy, and new datastore support
- add watch sync, named sessions, runtime tool policy, and new datastore support

## [0.6.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.5.0...appctl-v0.6.0) - 2026-04-23

### Fixed

- *(appctl)* harden tool diagnostics, improve app context flow, and clean up release docs

### Other

- format

## [0.5.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.4.0...appctl-v0.5.0) - 2026-04-23

### Added

- *(cli)* wire `init`, `app`, and `doctor models` commands

## [0.4.0](https://github.com/Esubaalew/appctl/compare/appctl-v0.3.0...appctl-v0.4.0) - 2026-04-23

### Fixed

- fix fmt and clippy failures

### Other

- *(ui,docs)* monochrome operator console, docs theme parity, honest CLI reference

## [0.1.0] - TBD

Initial release. See the workspace [`CHANGELOG.md`](../../CHANGELOG.md) for the full feature list.

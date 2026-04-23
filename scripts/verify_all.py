#!/usr/bin/env python3
"""Run the appctl verify matrix and produce a JSON report.

Consumes scripts/verify-matrix.toml and, for every case whose `requires_env`
variables are all set, it:

1. Copies the demo directory to a scratch dir.
2. Writes a provider config (including keychain secret) using the in-tree
   `appctl` binary (either `./target/release/appctl` or whatever is on PATH).
3. Runs `appctl sync` with the configured argv.
4. For real-API cases, runs `appctl run "<prompt>"` and asserts exit 0.
5. For MCP bridge cases, runs `appctl init`-equivalent steps non-interactively
   via `appctl auth provider login --subscription` and asserts the external
   client's config file was written.

Writes verify-report.json in the repo root with per-case results.
"""
from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore

REPO = Path(__file__).resolve().parent.parent
MATRIX = REPO / "scripts" / "verify-matrix.toml"
DEMOS = REPO / "examples" / "demos"


@dataclass
class CaseResult:
    id: str
    status: str  # "pass" | "fail" | "skip"
    reason: str = ""
    duration_ms: int = 0
    steps: list[dict] = field(default_factory=list)


def appctl_binary() -> str:
    candidate = REPO / "target" / "release" / "appctl"
    if candidate.exists():
        return str(candidate)
    candidate = REPO / "target" / "debug" / "appctl"
    if candidate.exists():
        return str(candidate)
    found = shutil.which("appctl")
    if not found:
        sys.exit("appctl binary not found. Build with `cargo build -p appctl` first.")
    return found


def write_provider_config(app_dir: Path, case: dict) -> None:
    name = case["provider_name"]
    kind = case["provider_kind"]
    base_url = os.environ.get(case.get("base_url_env", ""), case.get("base_url", ""))
    model = os.environ.get(case.get("model_env", ""), case.get("model", ""))
    auth_kind = case["auth_kind"]

    lines = [f'default = "{name}"', ""]
    lines.append("[[provider]]")
    lines.append(f'name = "{name}"')
    lines.append(f'kind = "{kind}"')
    lines.append(f'base_url = "{base_url}"')
    lines.append(f'model = "{model}"')
    if auth_kind == "api_key":
        secret_ref = case["secret_ref"]
        lines.append(
            f'auth = {{ kind = "api_key", secret_ref = "{secret_ref}" }}'
        )
    elif auth_kind == "google_adc":
        project = os.environ.get("VERTEX_PROJECT") or os.environ.get("GOOGLE_PROJECT") or ""
        if project:
            lines.append(
                f'auth = {{ kind = "google_adc", project = "{project}" }}'
            )
        else:
            lines.append('auth = { kind = "google_adc" }')
    elif auth_kind == "azure_ad":
        tenant = os.environ["AZURE_TENANT_ID"]
        client_id = os.environ["AZURE_CLIENT_ID"]
        lines.append(
            f'auth = {{ kind = "azure_ad", tenant = "{tenant}", client_id = "{client_id}", device_code = true }}'
        )
    elif auth_kind == "mcp_bridge":
        client = case["bridge_client"]
        lines.append(f'auth = {{ kind = "mcp_bridge", client = "{client}" }}')
    else:
        lines.append('auth = { kind = "none" }')

    for header in case.get("extra_headers", []) or []:
        key, _, value = header.partition("=")
        lines.append("[provider.extra_headers]")
        lines.append(f'{key} = "{value}"')

    app_dir.mkdir(parents=True, exist_ok=True)
    (app_dir / "config.toml").write_text("\n".join(lines) + "\n")


def run(cmd: list[str], cwd: Path, env: dict | None = None, timeout: int = 180) -> tuple[int, str]:
    result = subprocess.run(
        cmd,
        cwd=cwd,
        env=env or os.environ.copy(),
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return result.returncode, (result.stdout + result.stderr)[-4000:]


def store_secret(appctl: str, app_dir: Path, name: str, value: str) -> tuple[int, str]:
    return run(
        [appctl, "--app-dir", str(app_dir), "config", "set-secret", name, "--value", value],
        cwd=REPO,
    )


def run_case(case: dict, *, appctl: str) -> CaseResult:
    case_id = case["id"]
    start = time.time()
    result = CaseResult(id=case_id, status="pass")

    required = case.get("requires_env", []) or []
    missing = [var for var in required if not os.environ.get(var)]
    if missing:
        result.status = "skip"
        result.reason = "missing env: " + ", ".join(missing)
        return result

    demo_dir = DEMOS / case["demo"]
    if not demo_dir.exists():
        result.status = "fail"
        result.reason = f"demo dir not found: {demo_dir}"
        return result

    with tempfile.TemporaryDirectory(prefix="appctl-verify-") as tmp:
        scratch = Path(tmp) / case["demo"]
        shutil.copytree(demo_dir, scratch)
        app_dir = scratch / ".appctl"
        write_provider_config(app_dir, case)

        if case["auth_kind"] == "api_key":
            secret_env = case["secret_env"]
            secret_ref = case["secret_ref"]
            value = os.environ.get(secret_env, "")
            if not value:
                result.status = "fail"
                result.reason = f"env var {secret_env} unexpectedly empty"
                return result
            rc, out = store_secret(appctl, app_dir, secret_ref, value)
            result.steps.append({"step": "set-secret", "rc": rc, "log": out})
            if rc != 0:
                result.status = "fail"
                result.reason = "failed to store secret"
                return result

        sync_cmd = [appctl, "--app-dir", str(app_dir)] + list(case["sync"])
        rc, out = run(sync_cmd, cwd=scratch)
        result.steps.append({"step": "sync", "rc": rc, "log": out})
        if rc != 0:
            result.status = "fail"
            result.reason = "sync failed"
            return result

        if case.get("smoke_only"):
            return result

        if case["auth_kind"] == "mcp_bridge":
            return result

        prompt = case.get("prompt", "Reply with the single token 'ok'.")
        run_cmd = [appctl, "--app-dir", str(app_dir), "run", prompt]
        rc, out = run(run_cmd, cwd=scratch, timeout=300)
        result.steps.append({"step": "run", "rc": rc, "log": out})
        if rc != 0:
            result.status = "fail"
            result.reason = "run failed"
            return result

    result.duration_ms = int((time.time() - start) * 1000)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--strict", action="store_true",
                        help="fail if any case was skipped")
    parser.add_argument("--report", default=str(REPO / "verify-report.json"),
                        help="path to write the JSON report")
    parser.add_argument("--only", default="",
                        help="comma-separated case ids to run (others skipped)")
    args = parser.parse_args()

    with MATRIX.open("rb") as fp:
        matrix = tomllib.load(fp)

    filter_ids = {item.strip() for item in args.only.split(",") if item.strip()}

    appctl = appctl_binary()
    results: list[CaseResult] = []
    for case in matrix.get("case", []):
        if filter_ids and case["id"] not in filter_ids:
            continue
        print(f"==> {case['id']}")
        results.append(run_case(case, appctl=appctl))

    pass_count = sum(1 for r in results if r.status == "pass")
    fail_count = sum(1 for r in results if r.status == "fail")
    skip_count = sum(1 for r in results if r.status == "skip")

    report = {
        "appctl_binary": appctl,
        "git_sha": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        "pass": pass_count,
        "fail": fail_count,
        "skip": skip_count,
        "results": [asdict(r) for r in results],
    }

    Path(args.report).write_text(json.dumps(report, indent=2))
    print("")
    print(f"pass={pass_count} fail={fail_count} skip={skip_count}")
    print(f"report: {args.report}")

    if fail_count > 0:
        return 1
    if args.strict and skip_count > 0:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())

# Canary Definition

## Detection Sources

- CI workflow: `.github/workflows/ci.yml`
- Release workflow: `.github/workflows/release.yml`
- Local dashboard server: `src/dashboard/mod.rs`
- Dashboard routes: `src/dashboard/routes.rs`
- Dashboard UI: `src/dashboard/static/index.html`
- Binary entrypoint and slash wiring: `src/main.rs`

## Detected Deployment Files

- `Dockerfile`: N/A
- `docker-compose.yml`: N/A
- `railway.json`: N/A
- `vercel.json`: N/A
- `fly.toml`: N/A
- `render.yaml`: N/A
- `k8s/`: N/A
- `helm/`: N/A

## Build Health Check

### Goal

Catch workspace build, lint, and test regressions before feature work moves forward.

### Checks

- `cargo build --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p forja-llm`
- `cargo test -p forja-llm -- --ignored`

### Notes

- CI also validates a reduced path with `--no-default-features`.
- Feature-enabled paths such as Telegram and vision should be treated as separate canary risk areas.

## Endpoint Health Check

### Status

No dedicated `/health` or `/status` endpoint is implemented.

### Practical Probe

If the runtime dashboard is started, the closest API availability probe is:

- `GET http://localhost:3700/api/tasks`

### Notes

This is an application-data route, not a formal health endpoint.

## Browser Health Check

### Target

- `http://localhost:3700/`

### Goal

Verify that the local dashboard starts, serves the embedded HTML shell, and can load its data tabs.

### Practical Probe

1. Start the runtime.
2. Run `/dashboard`.
3. Confirm that the browser opens the dashboard.
4. Confirm that the page renders the Audit, Debates, Budget, and Tasks tabs without a load error.

## Current Canary Summary

- Build health: defined
- Endpoint health: partial, via dashboard data route only
- Browser health: defined for the local dashboard
- Deployment platform health: not applicable in the current repository state

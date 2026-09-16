---
name: ign-rigs
description: Test-rig lifecycle with ign — Docker compose rigs for Ignition 8.3 (up/down/reset/status), OIDC trial resets for licensed testing, and commissioning waits. Use when spinning up disposable Ignition gateways for development or verification.
user-invocable: false
---

# ign Test Rigs

Disposable Ignition 8.3 gateways via Docker Compose — the harness behind every live verification in this project. macOS + Linux only (Docker required; no Windows CLI build).

## Lifecycle

```bash
ign rig up               # compose discovery → port pre-flight → up → commissioned-wait
ign rig status           # state, ports, health
ign rig logs [-f]
ign rig down
ign rig reset --yes      # fresh volumes — destructive (guarded)
```

- **Compose discovery is 5-level**: the CLI finds the compose file walking up/down the usual layout — run `rig` verbs from inside or beside the rig directory.
- `rig up` waits for the containers to reach running — but a **fresh volume terminally reports exit 0 with `state: "uncommissioned"`** and the wizard URL in `warnings`. Commissioning is NOT automatic: check the result for `state`, and when it is `"uncommissioned"`, complete the `/welcome` wizard (URL from the warnings) **before** running commissioned-only operations. `ign doctor`'s commissioning check is the oracle.
- `rig up` blocks until the gateway answers — never hand-poll HTTP; the wait is built in. Commissioning is the one state it does not guarantee.
- `rig` verbs need **no profile** — they talk to Docker, not the gateway.

## Trial state (2-hour licensed windows)

Fresh rigs boot into an OIDC trial — 2 hours of licensed behavior (historian, EAM controller, etc.) per window:

```bash
ign rig trial --json          # trial state + remaining countdown
ign rig trial reset --yes     # native OIDC reset — guarded; refused exit 2 without --yes
```

Budget verification work to fit inside a window: a full live-suite run comfortably fits in one; commissioning a fresh rig takes ~4 minutes.

## Gateway backups (gwbk)

```bash
ign backup download --out pre-change.gwbk --json    # streamed gwbk on any profiled gateway
ign backup restore --file pre-change.gwbk --yes --json   # guarded
```

## Rig discipline (why rigs exist)

- **Never verify against production.** Guarded-write verification (EAM task lifecycle, imports) runs on disposable rigs; the live-gate pattern is env-gated so CI skips cleanly when no rig is up.
- Trial windows and rig churn are cheap — when a rig behaves oddly, `rig reset --yes` before diagnosing. A wedged first boot can leave a dead historian storage engine; fresh volumes beat forensics.
- Unique names per run (providers, tags, tasks): parallel or repeated verification sessions must not collide — several gateway behaviors (deploy model rebuilds, historian registration) only settle for names that have never been seen before.

## Cross-platform notes

- Docker Desktop / Docker Engine on macOS (arm64 + x86_64) and Linux (x86_64 + arm64). The rig images are linux/amd64 — Apple Silicon runs them under Rosetta emulation via Docker Desktop (set `DOCKER_DEFAULT_PLATFORM=linux/amd64` if compose doesn't pin it).
- `ign doctor` includes a rig check (Docker version discovery) — run it first when `rig up` misbehaves.

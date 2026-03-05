# Decision Log

Decisions made during development, with context and rationale.

## 2026-03-04: Headless auth via `claude auth login` (CURRENT)

**Context:** Users need to authenticate with Claude Code once, and all projects should pick up the credentials automatically. Earlier attempts used a PTY-based auth terminal window, but React StrictMode double-mount race conditions made the approach unreliable.

**Decision:** Run `claude auth login` headlessly in a temporary container. The flow:

1. User clicks "Sign in to Claude" on the dashboard.
2. Terrarium starts a temporary auth container (`terrarium-auth`) with `--network host` and `BROWSER=host-open`, but WITHOUT bind-mounting `~/.claude/` (critical — an empty bind-mount prevents Claude Code from writing credentials).
3. `claude auth login` runs via `nerdctl exec`. It opens the browser on the host via `host-open`, the user authorizes, and Claude Code's internal callback server receives the OAuth response. Lima's port forwarding handles the random callback port transparently.
4. After login succeeds, Terrarium copies `.credentials.json` from the container's `~/.claude/` to the shared auth dir (`/opt/terrarium/claude-auth/`) on the VM.
5. The auth container is removed.
6. When opening a terminal, `terminal_command` seeds `.credentials.json` from the shared auth dir into the dev container's `~/.claude/` (via piped `tee`) if not already present.

**Key discovery:** Bind-mounting an empty directory over `~/.claude/` prevents `claude auth login` from writing `.credentials.json`. The image's built-in `~/.claude/` (containing `settings.json`) must be preserved during auth. The auth container therefore uses NO bind mounts for `~/.claude/`.

**Rationale:** Headless auth is simpler and more reliable than PTY-based auth. No terminal window needed, no race conditions, no React lifecycle issues. The user just clicks a button and authorizes in their browser.

## 2026-03-04: Skip Claude Code onboarding in dev containers

**Context:** Even with credentials seeded, Claude Code runs a first-run onboarding flow (text style selection, login method choice) and a workspace trust dialog on every new container.

**Decision:** Bake onboarding-complete flags into the dev image's `.claude.json`:
- `hasCompletedOnboarding: true` — skips the first-run setup wizard.
- `firstStartTime` — marks Claude Code as previously launched.
- `numStartups: 1` — indicates at least one prior startup.
- `projects["/home/terrarium/workspace"].hasTrustDialogAccepted: true` — skips the workspace trust prompt.

These are set in the Dockerfile alongside the existing MCP server config.

**Rationale:** Terrarium controls the dev container environment and trusts it by definition. The onboarding flow is designed for interactive CLI users, not managed environments. Skipping it provides a seamless experience where the user goes straight from clicking "Open" to an active Claude Code session.

## 2026-03-04: Named volume for Claude Code config persistence

**Context:** Claude Code's configuration in `~/.claude/` is lost when a dev container is recreated (e.g., after an image rebuild).

**Decision:** Mount a named volume (`claude-config`) at `/home/terrarium/.claude` in dev containers. The volume name is scoped to the project's containerd namespace (`terrarium-{project_id}`), so each project has independent config. Volumes are cleaned up in `delete_namespace()` alongside containers and images.

**Rationale:** On first container creation, containerd auto-populates the named volume from the image's `~/.claude/` contents, seeding `settings.json` (MCP tool pre-approvals). Auth credentials are seeded separately from the shared auth dir at terminal open time. Per-project sessions, conversation history, and settings persist across container restarts and rebuilds.

## 2026-03-04: No BROWSER proxy for Claude Code OAuth (SUPERSEDED)

**Superseded by:** "Headless auth via `claude auth login`" above. Auth is now handled headlessly from a temporary container — no terminal window or PTY needed. Dev containers don't set `BROWSER`; only the temporary auth container does.

## 2026-03-04: Host API server on port 7778

**Context:** Need a communication channel from containers back to the desktop app for future MCP resource requests (Section 5.2 of the architecture doc).

**Decision:** Run an axum HTTP server on `0.0.0.0:7778` inside the Tauri app. The VM gateway IP is detected dynamically via `ip route show default` and passed to containers as `TERRARIUM_HOST_API`.

**Trade-offs:** Binding on `0.0.0.0` exposes the port on all host interfaces. Acceptable for a local dev tool — can add token auth later if needed.

## 2026-03-04: Tech stack — Tauri (Rust + TypeScript)

**Context:** Needed a desktop app framework for macOS. Candidates were Tauri, Electron, and Swift/AppKit.

**Decision:** Tauri with Rust backend and TypeScript/React frontend.

**Rationale:** Open source friendly (MIT/Apache 2.0), smaller binary than Electron, Rust backend is a good fit for system-level container management. Cross-platform potential if needed later.

## 2026-03-04: Lima + containerd for container runtime

**Context:** Need containerized dev environments on macOS.

**Decision:** Lima with Apple Virtualization.framework (VZ) backend, containerd/nerdctl inside the VM.

**Rationale:** Lima is Apache 2.0, lightweight, and uses Apple's native virtualization. Single shared VM (`terrarium`) with per-project containerd namespaces keeps resource usage low.

# Decision Log

Decisions made during development, with context and rationale.

## 2026-03-04: No BROWSER proxy for Claude Code OAuth

**Context:** Claude Code's OAuth flow starts a temporary HTTP server on a random port inside the container (e.g., `localhost:36093/callback`). We built a host API server (axum on port 7778) and a `host-open` script that could forward browser-open requests from the container to the host via `POST /open-url`.

**Problem:** Opening the auth URL on the host works, but the OAuth callback redirects to `localhost:<random-port>`, which hits the host — not the container. Forwarding the callback back would require dynamically detecting the port, setting up a two-hop TCP tunnel (host → VM → container), and tearing it down after auth completes.

**Decision:** Don't set the `BROWSER` env var in dev containers. Claude Code uses its default copy/paste auth flow. The host API server remains running for future MCP resource requests.

**Rationale:** Auth is a one-time operation per container. The complexity of dynamic port forwarding isn't justified. The host API infrastructure (`/open-url` endpoint, gateway IP detection) is still in place and can be re-enabled if the callback routing problem is solved later.

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

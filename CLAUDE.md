# Terrarium

*A safe little world for your creations.*

Terrarium is a macOS desktop app that serves as a companion to Claude Code. It manages the full lifecycle of AI-generated web applications — sandboxed development environments, local runtime, and deployment — so that non-programmers can actually run, share, and deploy the things they build with Claude Code.

Read `terrarium-architecture.md` for the full architecture document. It is the source of truth for all design decisions.

## Key Concepts

- **Every project runs in containers** (Lima + containerd on Apple's Virtualization.framework). Containers are the universal abstraction for dev and deploy.
- **Claude Code runs inside the dev container** and communicates with the desktop app via an MCP server. It requests resources declaratively (ports, databases, volumes, UI buttons) and Terrarium provisions them.
- **Dev containers are permissive but local-only** (localhost + LAN). Deploy containers are hardened (no Claude Code, no MCP, no shell) and can face the internet. This is the core security boundary.
- **The project manifest** (`terrarium-manifest.yaml`) is the source of truth for what a project needs. It lives on the host, not inside the container. MCP calls update it, and the Resource Manager reconciles it with the running containers.

## Tech Stack

- **Desktop app:** TBD — discuss with user before choosing. Candidates: Tauri (Rust + web UI), Swift/AppKit, Electron. Consider that the project is open source and macOS-only for now but may go cross-platform later.
- **MCP server:** TBD — discuss with user. Likely TypeScript (aligns with the MCP SDK ecosystem) or Go (lightweight, single binary, good for running inside containers).
- **Container runtime:** Lima + containerd/nerdctl, Apple Virtualization.framework (VZ) backend. This is decided.
- **Reverse proxy:** TBD — needs to handle `*.terrarium.local` routing and local TLS termination. Caddy is a strong candidate (automatic HTTPS, simple config).

## Current Phase

**Phase 1 — Foundation (MVP)**

See Section 10 of the architecture doc. The deliverables are:

1. macOS desktop app shell with a project dashboard (list of projects, create/delete).
2. Lima integration — programmatically create and manage a shared VM with containerd.
3. Dev container provisioning — spin up a minimal container with Claude Code and the MCP server pre-installed.
4. MCP server with core resource tools (add/remove containers, allocate ports, create volumes).
5. Project manifest — create, persist, and load project configuration.
6. Terminal window — open a Claude Code session inside the dev container, persistent via tmux. Closeable and reopenable without losing state.
7. `TERRARIUM.md` — static orientation file baked into the base dev image.

## How to Work on This Project

1. **Read the architecture doc first.** Every section matters. Don't skim it.
2. **Propose before building.** Before writing code for a new component, outline your plan and confirm with the user. This includes tech stack choices, file structure, and integration approach.
3. **Build incrementally.** One component at a time. Get it working, confirm with the user, then move on. Do not try to build the entire Phase 1 in one shot.
4. **Suggested build order for Phase 1:**
   - Start with the desktop app shell (project dashboard UI with stub data).
   - Then Lima integration (create/start/stop a VM, run a container).
   - Then dev container provisioning (base image, Claude Code install, tmux session).
   - Then the MCP server (start with `terrarium.env.info` and one resource tool like `allocatePort`).
   - Then wire it all together (creating a project in the UI provisions a real container).
   - Then the terminal window (open Claude Code inside the container from the UI).
5. **Test as you go.** Each component should be testable in isolation before integration.
6. **Keep the runtime abstract.** The container runtime should be behind an internal interface (see Section 3 of the architecture doc) so it could be swapped later, even though we're committed to Lima for v1.

## Important Constraints

- **Security defaults matter.** Even in Phase 1, containers should be sandboxed. Don't take shortcuts that undermine isolation — these are hard to retrofit.
- **No secrets in containers.** The secret proxy pattern (Section 6.4 of the architecture doc) is a later phase, but don't build anything now that would require secrets inside the container later.
- **Dev containers are never publicly exposed.** Only deploy containers (Phase 4+) can face the internet.
- **The MCP server runs inside the dev container.** It communicates with the desktop app via a host-reachable channel (socket, TCP port, or vsock — determine during Lima integration).
- **The project manifest lives on the host.** Not inside the container filesystem. It is the desktop app's responsibility to persist and manage it.
- **`TERRARIUM.md` is static.** It describes available MCP tools and best practices. It does not contain live project state. Claude Code calls `terrarium.env.info` for dynamic information.
- **This is open source.** No commercial dependencies in the core. Lima is Apache 2.0. All dependencies should have compatible licenses.

## Repository Structure

TBD — propose a structure based on the chosen tech stack. At minimum, expect top-level directories for:

- The desktop app
- The MCP server
- The base dev container image definition
- The `TERRARIUM.md` template
- Documentation (the architecture doc lives here)
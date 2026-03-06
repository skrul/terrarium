# Terrarium

*A safe little world for your creations.*

Terrarium is a macOS desktop app that serves as a companion to Claude Code. It manages the full lifecycle of AI-generated web applications — sandboxed development environments, local runtime, and deployment — so that non-programmers can actually run, share, and deploy the things they build with Claude Code.

Read `terrarium-architecture.md` for the full architecture document. It is the source of truth for all design decisions.

## Key Concepts

- **Every project runs in containers** (Lima + containerd on Apple's Virtualization.framework). Containers are the universal abstraction for dev and deploy.
- **Claude Code runs on the host** (Desktop app or CLI), not inside the container. Terrarium configures each project workspace with hooks that transparently proxy shell commands into the dev container. File operations work directly on the shared filesystem.
- **Dev containers are permissive but local-only** (localhost + LAN). Deploy containers are hardened (no dev tools, no shell) and can face the internet. This is the core security boundary.
- **The MCP server runs on the host** as a stdio subprocess of Claude Code. It communicates with the Terrarium desktop app via the host API (HTTP on port 7778) to provision resources.
- **Project workspaces live at `~/Terrarium/<project-name>/`** on the host. Lima's virtiofs makes them visible inside the container at `/home/terrarium/workspace/`.

## Tech Stack

- **Desktop app:** Tauri 2 (Rust backend + React/TypeScript frontend with TailwindCSS).
- **MCP server:** TypeScript using `@modelcontextprotocol/sdk`. Runs on the host as a stdio subprocess of Claude Code (configured in each project's `.mcp.json`).
- **Container runtime:** Lima + containerd/nerdctl, Apple Virtualization.framework (VZ) backend. This is decided.
- **Reverse proxy:** TBD — needs to handle `*.terrarium.local` routing and local TLS termination. Caddy is a strong candidate.

## Current Phase

**Phase 1 — Foundation (MVP)**

See Section 10 of the architecture doc. The deliverables are:

1. macOS desktop app shell with a project dashboard (list of projects, create/delete).
2. Lima integration — programmatically create and manage a shared VM with containerd.
3. Dev container provisioning — spin up a container with workspace bind-mount.
4. Hooks-based command proxying — `PreToolUse` hook transparently routes Bash commands into the container.
5. MCP server with core resource tools (env info, allocate/list/release ports).
6. Project workspace setup — `~/Terrarium/<name>/` with `.claude/settings.json`, `.mcp.json`, `.terrarium/config.json`, `.claude/CLAUDE.md`.
7. "Open Terminal" button — opens a terminal at the project workspace for the user to run Claude Code.

## How Claude Code Integration Works

### File Operations
Claude Code operates directly on `~/Terrarium/<project>/` on the host. Lima's virtiofs makes these files immediately visible inside the container at `/home/terrarium/workspace/`. No hook needed — Read, Write, Edit, Glob, Grep all work natively.

### Bash Commands
A `PreToolUse` hook on `Bash` (defined in `.claude/settings.json`) intercepts every shell command. The hook script (`hooks/terrarium-proxy.sh`):
1. Reads `.terrarium/config.json` to find the container name
2. Maps the host `cwd` to the container workspace path
3. Writes the command to a temp file (avoids shell quoting issues with limactl SSH)
4. Rewrites the command to: `cat <tmpfile> | limactl shell terrarium -- sudo nerdctl exec -i --user terrarium -w <cwd> <container> bash -l`
5. Returns `permissionDecision: "allow"` with the rewritten command

### MCP Server
The Terrarium MCP server runs on the host as a stdio subprocess of Claude Code (configured in `.mcp.json`). It communicates with the Terrarium desktop app via the host API (HTTP on port 7778) to provision resources, get project status, etc.

## How to Work on This Project

1. **Read the architecture doc first.** Every section matters. Don't skim it.
2. **Propose before building.** Before writing code for a new component, outline your plan and confirm with the user. This includes tech stack choices, file structure, and integration approach.
3. **Build incrementally.** One component at a time. Get it working, confirm with the user, then move on.
4. **Test as you go.** Each component should be testable in isolation before integration.

## Important Constraints

- **Security defaults matter.** Even in Phase 1, containers should be sandboxed. Don't take shortcuts that undermine isolation — these are hard to retrofit.
- **No secrets in containers.** The secret proxy pattern (Section 6.4 of the architecture doc) is a later phase, but don't build anything now that would require secrets inside the container later.
- **Dev containers are never publicly exposed.** Only deploy containers (Phase 4+) can face the internet.
- **The project workspace lives on the host.** At `~/Terrarium/<project-name>/`. The `.terrarium/config.json` file stores project metadata (project ID, container name).
- **This is open source.** No commercial dependencies in the core. Lima is Apache 2.0. All dependencies should have compatible licenses.
- **Keep MCP docs in sync.** When adding or changing MCP tools, you must update all three places: (1) the tool's `description` string in `mcp-server/src/index.ts`, (2) the per-project template at `desktop/src-tauri/templates/CLAUDE.md`, and (3) this file's description of MCP tools. Claude Code users rely on these descriptions to discover and use tools correctly.

## Repository Structure

```
terrarium/
├── desktop/                    # Tauri desktop app
│   ├── src/                    # React frontend
│   │   ├── App.tsx
│   │   ├── components/         # ProjectDashboard, ProjectCard, VmStatusBar, etc.
│   │   ├── hooks/              # useProjects, useVmStatus
│   │   └── types/              # TypeScript type definitions
│   └── src-tauri/              # Rust backend
│       ├── src/
│       │   ├── lib.rs          # Tauri commands, project creation, workspace setup
│       │   ├── error.rs        # Error types
│       │   ├── project.rs      # Project struct and status enum
│       │   ├── host_api.rs     # Host API server (port 7778)
│       │   └── runtime/        # Container runtime abstraction
│       │       ├── mod.rs      # ContainerRuntime trait
│       │       ├── lima.rs     # Lima implementation
│       │       └── types.rs    # VmStatus, ContainerStatus, RuntimeStatus
│       ├── hooks/
│       │   └── terrarium-proxy.sh  # PreToolUse hook for Bash command proxying
│       ├── lima-terrarium.yaml     # Lima VM template
│       └── Dockerfile.dev-base    # Dev container base image
├── mcp-server/                 # Terrarium MCP server (TypeScript)
│   ├── src/
│   │   ├── index.ts            # Server entry point
│   │   └── tools/              # MCP tool implementations
│   └── dist/
│       └── terrarium-mcp.js    # Built output (single file via esbuild)
└── terrarium-architecture.md   # Full architecture document
```

## Per-Project Workspace Structure

When a project is created, Terrarium sets up `~/Terrarium/<project-name>/`:

```
~/Terrarium/<project-name>/
├── .claude/
│   ├── settings.json       # Hooks (Bash proxy) + permissions
│   ├── settings.local.json # Pre-approved MCP servers
│   └── CLAUDE.md           # Project instructions for Claude Code
├── .terrarium/
│   └── config.json         # Project ID, container name, created timestamp
├── .mcp.json               # MCP server config (terrarium server)
└── <user's project files>  # Created by Claude Code during development
```

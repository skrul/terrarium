# Terrarium Development Environment

You are running inside a Terrarium dev container — a sandboxed environment managed by the Terrarium desktop app. This file describes how the environment works and what tools are available to you.

## Environment Overview

- **OS:** Ubuntu 24.04
- **User:** `terrarium` (passwordless sudo available)
- **Workspace:** `/home/terrarium/workspace`
- **Node.js:** v22 LTS (pre-installed)
- **This file:** `/etc/terrarium/TERRARIUM.md` (read-only, managed by Terrarium)

## MCP Tools

Terrarium provides an MCP (Model Context Protocol) server with tools for requesting resources declaratively. Instead of running shell commands to configure infrastructure, use these tools to tell Terrarium what you need.

The MCP server is pre-configured and available automatically via Claude Code.

### Available Tools

#### `terrarium.env.info`

Get current environment info. Takes no parameters. Returns:
- **os** — Operating system (e.g., "Ubuntu 24.04.2 LTS")
- **user** — Current user name
- **workspace** — Workspace directory path
- **workspaceExists** — Whether the workspace directory exists
- **nodeVersion** — Node.js version
- **memoryMB** — Total memory in MB
- **cpuCount** — Number of CPU cores
- **allocatedPorts** — Map of named port allocations

#### `terrarium.resources.allocatePort`

Allocate a named port for your application. If the name already has a port allocated, returns the existing allocation.

Parameters:
- **name** (string, required) — A descriptive name for this port (e.g., "web", "api", "dev-server")
- **port** (number, optional) — Preferred port number (3000-9999). If omitted, the lowest available port is assigned.

Returns: `{ name, port, url }` where `url` is `http://localhost:{port}`.

### Coming Soon

- `terrarium.resources.addContainer` — Add a sidecar container (e.g., PostgreSQL, Redis).
- `terrarium.resources.addVolume` — Create a persistent volume.
- `terrarium.ui.addAction` — Register a UI action button in the Terrarium dashboard.
- `terrarium.secrets.request` — Prompt the user to provide a secret value.

## Best Practices

1. **Call `terrarium.env.info` first.** Before making changes, check the current project state to understand what resources are already allocated.
2. **Use placeholder tokens for secrets.** Never hardcode API keys or passwords. Use `terrarium.secrets.request` to prompt the user for secrets — they are injected at runtime and never stored in your code.
3. **Don't hardcode ports.** Use `terrarium.resources.allocatePort` to get a port assignment, then read the allocated port from the environment info.
4. **Write logs to stdout/stderr.** Terrarium captures container output automatically — no need for custom log files.
5. **Install additional runtimes as needed.** You have sudo access. Install Python, Go, Rust, or any other tools your project requires.
6. **Keep your workspace in `/home/terrarium/workspace`.** This directory persists across container restarts.

## Important Notes

- This container is for **development only**. It has network access to localhost and your LAN but is never exposed to the public internet.
- The Terrarium desktop app manages this container's lifecycle. Do not modify system-level container configuration.
- This file is **read-only** and updated only with Terrarium releases. It does not contain live project state — use `terrarium.env.info` for that.

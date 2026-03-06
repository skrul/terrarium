# Terrarium — Architecture Document

*A safe little world for your creations.*

**Version:** 0.2
**Date:** March 2026

---

## 1. Vision

Vibe coding tools like Claude Code have made it possible for non-programmers to build real web applications through conversation. But the last mile — running, debugging, deploying, and sharing those apps — still requires developer-level skills. Terrarium bridges that gap.

Terrarium is a macOS desktop application that serves as a companion to Claude Code. It manages the full lifecycle of AI-generated web applications: providing sandboxed development environments, a simple local runtime, and a path to deployment — all without requiring the user to touch a terminal, write a Dockerfile, or configure a cloud provider.

The core metaphor: each project is a sealed terrarium — a self-contained little world where your app lives, grows, and runs safely, isolated from your host system and from other projects.

### Design Principles

- **Containers as the universal abstraction.** Every project runs in containers, whether in development or production. This gives us sandboxing, portability, and a consistent deployment target from laptop to cloud.
- **Claude Code is a first-class citizen.** Rather than Claude Code fumbling with shell commands, it communicates with Terrarium through a structured MCP interface to declare what it needs. Terrarium provisions the resources.
- **Declarative, not imperative.** Claude Code describes *what* the project needs (a database, a port, a log stream) and Terrarium figures out *how* to provide it. Configuration is persisted as a project manifest, not as a pile of shell history.
- **Secure by default.** All LLM-generated code is assumed to be insecure. Secrets never enter the container. Network egress is restricted. The host filesystem is off-limits unless explicitly granted. A compromised app can't escape its terrarium.
- **Progressive disclosure.** A first-time user should be able to go from zero to a running app with a few clicks. Advanced features (cloud deployment, network policies, resource tuning) are available but never required.

---

## 2. High-Level Architecture

```
┌─────────────────────────────────────────┐
│ Terrarium Desktop App (Tauri)           │
│  • Project lifecycle management         │
│  • Lima VM management                   │
│  • Container create/start/stop          │
│  • Workspace directory setup            │
│  • .claude/settings.json generation     │
│  • Host API server (port 7778)          │
└──────────────┬──────────────────────────┘
               │ creates workspace at
               │ ~/Terrarium/<project>/
               │
┌──────────────▼──────────────────────────┐
│ Host Filesystem                          │
│ ~/Terrarium/<project>/                   │
│  ├── .claude/settings.json  (hooks)     │
│  ├── .claude/CLAUDE.md                  │
│  ├── .terrarium/config.json             │
│  ├── .mcp.json                          │
│  └── <user's project files>            │
└──────────────┬──────────────────────────┘
               │ virtiofs (writable)
┌──────────────▼──────────────────────────┐
│ Lima VM                                  │
│  └── Container                           │
│       /home/terrarium/workspace/         │
│       (bind-mount from host dir)         │
│       • Node.js, Python, etc.           │
└──────────────────────────────────────────┘

Claude Code (Desktop app or CLI)
  • User opens ~/Terrarium/<project>/
  • File ops work directly (shared fs)
  • Bash commands proxied via hook →
    limactl shell → nerdctl exec → container
  • MCP server runs on host as stdio
    subprocess, talks to Terrarium app
    via host API (port 7778)
```

### Component Summary

| Component | Responsibility |
|---|---|
| **Project Manager** | CRUD for projects. Creates workspace directories at `~/Terrarium/<name>/` with hooks, MCP config, and project metadata. Manages project lifecycle (create, start, stop, delete). |
| **Resource Manager** | Translates resource declarations from the manifest into container configuration. Handles ports, volumes, sidecar containers, memory limits, cron jobs, etc. |
| **Container Runtime** | Lima with containerd/nerdctl on Apple's Virtualization.framework. Runs a shared VM. Manages dev containers with workspace bind-mounts, image builds, and container lifecycle. |
| **Reverse Proxy** | Routes `*.terrarium.local` hostnames to the correct container port. Handles TLS termination with a local CA. Supports both desktop and LAN access. *(Future)* |
| **MCP Server** | Runs on the **host** as a stdio subprocess of Claude Code (configured in `.mcp.json`). Exposes tools that let Claude Code declare resources, stream logs, configure the project UI, and trigger deployments. Communicates with the Terrarium desktop app via the host API (HTTP on port 7778). Not present in deploy containers. |
| **Hook Script** | `terrarium-proxy.sh` — a `PreToolUse` hook that intercepts Bash commands and rewrites them to execute inside the dev container via `limactl shell → nerdctl exec`. This is the core mechanism that makes Claude Code's shell commands run in the sandbox. |
| **Secret Proxy** | Intercepts outbound HTTP(S) from containers and substitutes placeholder tokens with real secrets. Secrets never exist inside the container filesystem or environment. *(Future)* |
| **Deploy Engine** | Builds OCI images from project state, pushes to local or remote targets. *(Future)* |
| **Audit Log** | Records every MCP call, resource allocation, port opening, and deployment. *(Future)* |

---

## 3. Container Runtime

### Requirements

The container runtime must provide:

1. **VM-level isolation.** Containers share a lightweight VM, not the host kernel. This is non-negotiable given that we're running untrusted code. A container escape should not give access to the host.
2. **OCI compatibility.** Projects produce standard OCI images so they can be deployed to any cloud provider without modification.
3. **Fast startup.** Users will create and destroy dev environments frequently. Cold start should be under 5 seconds.
4. **Low resource overhead.** This runs on developer laptops. The runtime itself should use minimal memory and CPU when idle.
5. **Container groups.** A project may consist of multiple containers (app + database + cache) that share a network namespace but are otherwise isolated from each other and from other projects.
6. **macOS native.** Must run well on Apple Silicon Macs.

### Runtime: Lima

The container runtime is **Lima** using **Apple's Virtualization.framework** (VZ) as the backend, with **containerd** and **nerdctl** for container management.

This gives us:

- **VM-level isolation** with near-native performance on Apple Silicon. Containers run inside a lightweight Linux VM, not on the host kernel, which is the foundation of our security model.
- **Full OCI compatibility** via containerd/nerdctl. Projects produce standard OCI images that can be pushed to any registry and deployed to any cloud provider.
- **No commercial licensing concerns.** Lima is open source (Apache 2.0). We control the full stack with no dependency on a commercial vendor.
- **Apple Virtualization.framework integration.** VZ provides hardware-accelerated virtualization on ARM Macs with Rosetta support for x86 images, shared filesystem via virtio-fs, and low overhead.

Terrarium will manage Lima instances programmatically via its CLI or socket API. Each project's container group runs within a shared Lima VM (to avoid per-project VM overhead), with containerd namespaces providing project-level isolation within the VM.

The runtime should still be abstracted behind an internal interface so we could swap implementations later if needed, but Lima is the committed choice for v1.

### Container Runtime Interface (Internal)

```
ContainerRuntime
  ├── createContainerGroup(projectId, manifest) → GroupHandle
  ├── startGroup(groupHandle)
  ├── stopGroup(groupHandle)
  ├── destroyGroup(groupHandle)
  ├── addContainer(groupHandle, imageRef, config) → ContainerId
  ├── removeContainer(groupHandle, containerId)
  ├── execInContainer(containerId, command) → Stream
  ├── getContainerLogs(containerId, since?) → Stream
  ├── buildImage(contextPath, buildSpec) → ImageRef
  ├── pushImage(imageRef, registry)
  ├── listGroups() → GroupHandle[]
  └── getGroupStatus(groupHandle) → GroupStatus
```

---

## 4. Project Lifecycle

### 4.1 Project Manifest

Every project has a manifest that describes its current state. This is the source of truth for what the project needs. It is persisted by the desktop app (not inside the container filesystem) and is updated via MCP calls from Claude Code or direct user actions in the UI.

```yaml
# Example: terrarium-manifest.yaml
project:
  id: "proj_abc123"
  name: "My Recipe App"
  created: "2026-03-01T10:00:00Z"

environment:
  base_image: "terrarium/dev-base:latest"
  memory_limit: "2Gi"
  cpu_limit: 2
  claude_code:
    auto_update: true            # Update on every container restart (default)
    # pinned_version: "1.2.3"   # Set to disable auto-update and pin to a specific version

resources:
  containers:
    - name: "postgres"
      image: "postgres:16"
      environment:
        POSTGRES_DB: "recipes"
        POSTGRES_USER: "app"
        POSTGRES_PASSWORD: "${secret:postgres_password}"
      volumes:
        - name: "pg_data"
          mount: "/var/lib/postgresql/data"

  ports:
    - name: "web"
      container_port: 3000
      hostname: "recipe-app"       # → recipe-app.terrarium.local
      description: "Main web app"

  volumes:
    - name: "pg_data"
      size: "1Gi"
    - name: "uploads"
      size: "500Mi"

  host_mounts:
    - name: "photos"
      host_path: "~/Pictures/Recipes"
      container_path: "/mnt/photos"
      read_only: true

  cron:
    - name: "cleanup"
      schedule: "0 3 * * *"
      command: "node scripts/cleanup.js"

  log_sinks:
    - name: "app"
      source: "stdout"
      description: "Application logs"
    - name: "requests"
      source: "/var/log/app/requests.log"
      description: "HTTP request log"

ui:
  actions:
    - name: "Start Dev Server"
      command: "npm run dev"
      icon: "play"
      primary: true
    - name: "Run Migrations"
      command: "npx prisma migrate dev"
      icon: "database"
    - name: "Open App"
      type: "open_url"
      url: "https://recipe-app.terrarium.local"
      icon: "globe"
    - name: "Run Tests"
      command: "npm test"
      icon: "check"
  status:
    type: "process"
    watch_command: "npm run dev"
    healthy_pattern: "ready on port 3000"

deploy:
  entrypoint: "npm start"
  build_steps:
    - "npm ci --production"
    - "npx prisma generate"
  bootstrap_steps:            # Run once on first deploy only
    - "npx prisma migrate deploy"
    - "node scripts/seed.js"
  health_check:
    path: "/health"
    interval: 30
    timeout: 5
```

### 4.2 Creating a New Project

1. User clicks "New Project" in the desktop app.
2. User provides a name (editable later).
3. Terrarium creates the project workspace at `~/Terrarium/<name>/`:
   - `.claude/settings.json` — hooks configuration (Bash proxy) and auto-approved permissions.
   - `.claude/settings.local.json` — pre-approves MCP servers so no prompt appears.
   - `.claude/CLAUDE.md` — project-level instructions for Claude Code.
   - `.terrarium/config.json` — project ID, container name, creation timestamp.
   - `.mcp.json` — configures the Terrarium MCP server as a stdio subprocess.
4. Terrarium ensures the Lima VM is running and the dev base image is built.
5. Terrarium creates and starts a dev container with a writable bind-mount from the host workspace (`~/Terrarium/<name>/`) to `/home/terrarium/workspace/` inside the container.
6. The project appears in the dashboard in "Running" state.

### 4.3 Working in a Project

1. User clicks "Open Terminal" on the project card. This opens a new Terminal.app window at `~/Terrarium/<name>/`.
2. The user launches Claude Code (CLI: `claude`, or opens the directory in Claude Code Desktop).
3. Claude Code reads `.claude/CLAUDE.md` and discovers the Terrarium MCP server via `.mcp.json`.
4. **File operations** (Read, Write, Edit, Glob, Grep) work directly on the host filesystem. Changes are immediately visible inside the container via virtiofs.
5. **Bash commands** are transparently proxied into the container by the `PreToolUse` hook. Claude Code doesn't know it's running in a sandbox — the hook rewrites every shell command to execute via `limactl shell → nerdctl exec`.
6. As Claude Code builds, it can use MCP tools to request resources:
   - "I need a PostgreSQL database" → MCP call to add a sidecar container.
   - "The app runs on port 3000" → MCP call to allocate a named port.
7. Each MCP call communicates with the Terrarium desktop app via the host API, which updates the project manifest and reconciles with the running container.

### 4.4 Project States

```
  ┌──────────┐     ┌───────────┐     ┌─────────┐
  │ Creating │ ──→ │   Ready   │ ──→ │ Running │
  └──────────┘     └───────────┘     └─────────┘
                        ↑                  │
                        │                  ↓
                   ┌────┴────┐       ┌──────────┐
                   │ Stopped │ ←──── │  Error   │
                   └─────────┘       └──────────┘
                        │
                        ↓
                   ┌──────────┐
                   │ Archived │
                   └──────────┘
```

---

## 5. MCP Server

The MCP server is the primary interface between Claude Code and Terrarium. It runs on the **host** as a stdio subprocess of Claude Code, configured in each project's `.mcp.json` file. This means it starts automatically when Claude Code opens a Terrarium project directory.

The MCP server communicates with the Terrarium desktop app via the host API (HTTP on port 7778). This is how resource requests (add a container, allocate a port) flow from Claude Code → MCP server → Terrarium app → container runtime. The MCP server receives the project ID via the `TERRARIUM_PROJECT_ID` environment variable and the host API URL via `TERRARIUM_HOST_API`.

Note that the MCP server is only present in dev workflows — deployed containers have no MCP server and cannot request resource changes.

### 5.1 Tool Categories

#### Resource Management

| Tool | Description |
|---|---|
| `terrarium.resources.addContainer` | Add a sidecar container (e.g., postgres, redis, minio). Accepts image name, env vars, volume mounts, resource limits. |
| `terrarium.resources.removeContainer` | Remove a sidecar container by name. |
| `terrarium.resources.listContainers` | List all sidecar containers and their status. |
| `terrarium.resources.allocatePort` | Request an externally accessible port. Returns the assigned `*.terrarium.local` hostname. |
| `terrarium.resources.releasePort` | Release a previously allocated port. |
| `terrarium.resources.createVolume` | Create a named persistent volume with a size limit. |
| `terrarium.resources.deleteVolume` | Delete a named volume. |
| `terrarium.resources.requestHostMount` | Request read or read-write access to a host directory. Triggers a user approval prompt in the desktop app. |
| `terrarium.resources.setMemoryLimit` | Request a memory limit change for the dev container. |
| `terrarium.resources.setCpuLimit` | Request a CPU limit change for the dev container. |

#### Scheduling

| Tool | Description |
|---|---|
| `terrarium.cron.add` | Add a cron job with a name, schedule (cron syntax), and command. |
| `terrarium.cron.remove` | Remove a cron job by name. |
| `terrarium.cron.list` | List all cron jobs and their last run status. |

#### Logging

| Tool | Description |
|---|---|
| `terrarium.logs.createSink` | Create a named log sink. Can watch a file path, a command's stdout/stderr, or a container's output. |
| `terrarium.logs.removeSink` | Remove a log sink by name. |
| `terrarium.logs.listSinks` | List all log sinks and whether they are active. |
| `terrarium.logs.write` | Write a message directly to a named log sink (useful for build output, status messages, etc.). |

#### UI Configuration

| Tool | Description |
|---|---|
| `terrarium.ui.addAction` | Add a button to the project card. Configurable with: name, command (run in container), type (command, open_url, open_terminal), icon, and whether it is the primary action. |
| `terrarium.ui.removeAction` | Remove a UI action by name. |
| `terrarium.ui.setStatus` | Set the project's status display. Can be a simple state (running/stopped/error) or a process watcher that monitors a command and parses output for health. |
| `terrarium.ui.addLink` | Add a quick link (e.g., "Open App", "API Docs") to the project card. |
| `terrarium.ui.showNotification` | Show a desktop notification to the user (e.g., "Build complete", "Migration finished"). |
| `terrarium.ui.addMetric` | Register a metric panel (e.g., request count, error rate, memory usage) that the desktop app can poll or the app can push to. |

#### Secrets

| Tool | Description |
|---|---|
| `terrarium.secrets.request` | Declare that the project needs a secret by name (e.g., `ANTHROPIC_API_KEY`). Triggers a prompt in the desktop app for the user to provide the value. Returns a placeholder token that the app should use in its configuration. |
| `terrarium.secrets.list` | List declared secrets and whether they have been provided by the user. |
| `terrarium.secrets.remove` | Remove a secret declaration. |

#### Deployment

| Tool | Description |
|---|---|
| `terrarium.deploy.setEntrypoint` | Declare the command to run the app in production. |
| `terrarium.deploy.setBuildSteps` | Declare the build steps needed before running (install deps, compile, etc.). Run every time an image is built. |
| `terrarium.deploy.setBootstrapSteps` | Declare steps that run once on first deploy (database migrations, seed data, creating default accounts, populating lookup tables). These should be idempotent. |
| `terrarium.deploy.setHealthCheck` | Declare how to check if the app is healthy (HTTP path, command, TCP port). |
| `terrarium.deploy.pin` | Trigger a local deployment — snapshot the current state and deploy it as a stable instance alongside the dev container. |

#### Environment Info

| Tool | Description |
|---|---|
| `terrarium.env.info` | Returns information about the current environment: OS, available memory, CPU cores, installed runtimes, network config, project name, project ID. |
| `terrarium.env.openBrowser` | Open a URL in the user's default browser on the host. |

### 5.2 User Approval Flow

Some MCP calls require user approval before taking effect. These are actions that could affect the host system or expose sensitive information:

- `terrarium.resources.requestHostMount` — grants container access to a host directory.
- `terrarium.secrets.request` — prompts the user to enter a secret value.
- `terrarium.resources.allocatePort` — when binding to the LAN (not just localhost).

When Claude Code makes one of these calls, the MCP server sends a request to the desktop app, which shows a confirmation dialog to the user. The MCP call blocks until the user approves or denies. Claude Code receives the result and can proceed or adjust accordingly.

### 5.3 The Project CLAUDE.md File

Each project workspace includes a `.claude/CLAUDE.md` file that serves as Claude Code's orientation document. It is generated by Terrarium during project creation and describes:

- That this is a Terrarium project with a sandboxed container environment.
- How file operations work directly on the host while shell commands are proxied into the container.
- What dev tools are available in the container (Node.js, Python, etc.).
- Tips for verifying the container environment.

The file does **not** contain live project state. All dynamic information (allocated resources, current configuration) is accessed through MCP calls, keeping a clean separation between "how the system works" (static) and "what's currently happening" (dynamic).

---

## 6. Networking

### 6.1 Local DNS and Reverse Proxy

Every project that allocates a port gets a hostname under `*.terrarium.local`. The desktop app runs a lightweight reverse proxy that:

1. Listens on ports 80 and 443 on the host.
2. Routes incoming requests by hostname to the correct container port.
3. Terminates TLS using certificates from a local CA.

The local CA is created during Terrarium's first-run setup and installed into the macOS system trust store (with user permission). This allows browsers to connect to `https://recipe-app.terrarium.local` without certificate warnings.

For `.local` resolution, Terrarium registers hostnames via macOS's built-in mDNS (Bonjour). This means other devices on the same local network (e.g., a phone on WiFi) can also resolve and access the app. Alternatively, Terrarium can write to `/etc/resolver/terrarium.local` for a more reliable approach that doesn't depend on mDNS.

**Important:** Dev containers are only accessible via `*.terrarium.local` on localhost and the LAN. They are never exposed to the public internet. Public exposure (Tailscale Funnel, cloud deployment) is only available for deploy containers, which have been stripped of Claude Code and the MCP server (see Section 8.2).

### 6.2 Inter-Container Networking

Containers within a project share a network namespace (or an isolated bridge network). This means:

- The web app container can reach `postgres:5432` by container name.
- Containers in *different* projects are fully isolated from each other.
- Only ports explicitly allocated via `terrarium.resources.allocatePort` are accessible from the host or LAN.

### 6.3 Egress Filtering

By default, containers have **restricted outbound network access**. The default egress policy:

- **Allowed:** DNS resolution, HTTP/HTTPS to common package registries (npm, PyPI, crates.io, etc.), communication with the Terrarium MCP server and secret proxy.
- **Denied:** Everything else.

Claude Code can request additional egress rules via `terrarium.resources.allowEgress(domain)`, which the Resource Manager adds to the project's network policy. This could be extended to require user approval for particularly sensitive domains (e.g., allowing outbound to an arbitrary IP).

### 6.4 Secret Proxy

The secret proxy sits between the container's outbound traffic and the external network. It operates as an HTTP(S) proxy that:

1. Inspects outbound requests for placeholder tokens (e.g., `__TERRARIUM_SECRET_ANTHROPIC_KEY__`).
2. Substitutes the placeholder with the real secret value from the secure store.
3. Forwards the request to the destination.

This means:

- Secrets are never present in the container filesystem, environment variables, or process memory.
- If the container is compromised, an attacker sees only placeholder tokens.
- The proxy's secure store is on the host, outside the container's reach.
- All secret substitutions are logged in the audit log.

The proxy only substitutes secrets for domains that have been explicitly associated with that secret in the project configuration. This prevents a compromised app from sending a secret placeholder to an attacker-controlled server — the proxy would not recognize the domain and would pass the placeholder through unsubstituted.

---

## 7. Deployment

### 7.1 Deployment Model

All deployments produce and run OCI images. Critically, deploy images are **stripped of all development tooling** — no Claude Code, no MCP server, no shell (where feasible), no package managers. They contain only the app, its runtime dependencies, and the entrypoint. This is the key security boundary: dev containers are permissive but local-only; deploy containers are hardened and can be exposed to the network or internet.

The build process:

1. Start from the project's dev container state (or a specified commit/tag).
2. Run the declared build steps (`terrarium.deploy.setBuildSteps`).
3. Set the entrypoint (`terrarium.deploy.setEntrypoint`).
4. Package as an OCI image.
5. Push to the target (local daemon, remote Terrarium instance, or cloud registry).

On first launch of a new deploy:

1. Create fresh, empty data volumes.
2. Run bootstrap steps (`terrarium.deploy.setBootstrapSteps`) — migrations, seed data, default accounts, etc.
3. Start the entrypoint.

On subsequent launches, bootstrap steps are skipped and only the entrypoint runs.

### 7.2 Data Persistence

**Dev and deploy data are always separate.** Deploys start with empty data volumes — dev data never leaks into production. This prevents test data, debug records, and local experiments from polluting a running instance.

To populate a fresh deploy, Claude Code declares **bootstrap steps** that run once on first deploy. These are distinct from build steps:

| Step Type | When It Runs | Purpose | Examples |
|---|---|---|---|
| **Build steps** | Every image build | Prepare the app to run | `npm ci`, `prisma generate`, compile |
| **Bootstrap steps** | First deploy only | Initialize data | `prisma migrate deploy`, `node seed.js`, create admin user |
| **Entrypoint** | Every launch | Run the app | `npm start` |

Bootstrap steps should be idempotent — safe to re-run if something fails partway through. The deploy UI includes a "Re-run Bootstrap" action (with confirmation) that lets the user wipe data volumes and re-run bootstrap steps from scratch if needed.

A future enhancement could add **snapshot/restore**: export a data volume from dev as an archive, then import it into a deploy as a starting point. This covers the "I curated a specific dataset in dev and want production to start there" use case. But it would be an explicit, intentional action — never automatic.

The same image format is used for all deployment targets, so a locally tested deploy should behave identically on a remote or cloud target.

### 7.3 Local Deployment ("Pin")

The most common deployment: snapshot the current dev state and run it as a stable, independent instance on the same machine.

- The pinned deploy runs in its own container group, separate from the dev container.
- It gets its own `*.terrarium.local` hostname (e.g., `recipe-app-live.terrarium.local` vs `recipe-app-dev.terrarium.local`).
- It can be configured to auto-start when Terrarium launches.
- Sidecar containers (databases, etc.) are included and run with their own volumes (data is *not* shared between dev and deploy unless explicitly configured).
- The project card in the UI shows both the dev and deployed instances with independent status, logs, and controls.
- The deploy UI includes a "Re-run Bootstrap" action (with confirmation) that wipes data volumes and re-runs bootstrap steps from scratch.
- UI actions declared by Claude Code carry over to the deployed instance where applicable.

### 7.4 Remote Deployment

A remote deployment pushes the OCI image to another Terrarium instance running on a different machine (e.g., a Mac Mini on the LAN).

- The remote Terrarium instance exposes a deploy API (authenticated).
- The deploy engine pushes the image + manifest to the remote.
- The remote instance runs the app as if it were a local deploy.
- The originating desktop app can monitor the remote deploy's status, logs, and metrics through the remote API.
- This enables 24/7 uptime for apps that shouldn't depend on the developer's laptop being open.

Discovery of remote Terrarium instances on the LAN can use Bonjour/mDNS, or the user can manually add a remote target by IP/hostname.

### 7.5 Cloud Deployment (Future)

Cloud deployment pushes the OCI image to a cloud container service. Initial targets should prioritize simplicity:

| Provider | Service | Complexity | Notes |
|---|---|---|---|
| Fly.io | Fly Machines | Low | Push a container, get a URL. Great DX. |
| Railway | Railway | Low | Similar to Fly. Git or image-based deploys. |
| AWS | ECS Fargate / App Runner | Medium-High | Powerful but complex IAM/VPC setup. |
| GCP | Cloud Run | Medium | Good balance of simplicity and power. |

The user configures cloud credentials in Terrarium's settings (one-time setup). Deploys are then a single click, same as local deploys. The deploy engine handles image pushing, service creation, and health checking.

### 7.6 Tailscale Integration (Future)

An optional add-on for local or remote deployments that exposes the app to the public internet via Tailscale Funnel:

- User connects their Tailscale account to Terrarium.
- For any local or remote deploy, user can toggle "Make Public."
- Terrarium configures Tailscale Funnel to route a public hostname to the app's local port.
- This avoids the complexity of cloud deployment while still making apps accessible from anywhere.

---

## 8. Security Model

### 8.1 Threat Model

We assume:

- **LLM-generated code is insecure.** It may contain vulnerabilities (XSS, SQL injection, SSRF, etc.) that an attacker could exploit.
- **A compromised app will try to escape.** An attacker who exploits the app will attempt to access the host filesystem, other containers, the local network, and exfiltrate secrets.
- **The user is not a security expert.** They will not audit the generated code or configure firewalls. Terrarium must be secure by default.

### 8.2 Dev vs. Deploy: Two Security Profiles

This is the core architectural security decision. Rather than trying to fully harden a dev container that inherently needs broad capabilities (Claude Code, MCP, package managers, build tools), we accept that dev containers have a larger attack surface and compensate by strictly limiting their exposure. Security is enforced at the deployment boundary.

**Dev containers** are optimized for building. They contain dev tools (Node.js, Python, build essentials) and have broad egress access to package registries. Claude Code runs on the host and proxies shell commands into the container via hooks. The MCP server also runs on the host. Dev containers are **never exposed to the public internet**. Access is limited to localhost and the local network (via `*.terrarium.local`). The acceptable risk here is bounded: an attacker would need to already be on the user's LAN.

**Deploy containers** are optimized for running. They are built from a clean OCI image containing only the app and its runtime dependencies. They do **not** contain Claude Code, the MCP server, development tools, package managers, or a shell (where feasible). The filesystem is read-only except for designated data volumes. Egress is tightly restricted. These are the containers that can be exposed to the public internet via Tailscale, cloud deployment, or remote Terrarium instances.

This separation gives us a clean, defensible security boundary:

| Property | Dev Container | Deploy Container |
|---|---|---|
| Claude Code | On host (proxied via hooks) | **No** |
| MCP Server | On host (stdio subprocess) | **No** |
| Dev tools / shell | Yes | **No** (where feasible) |
| Package managers | Yes | **No** |
| Filesystem | Read-write | **Read-only** (except data volumes) |
| Egress | Broad (registries, CDNs, user-approved domains) | **Minimal** (only domains the app needs) |
| Network exposure | Localhost + LAN only | Can be public internet |
| Secret handling | Placeholder tokens via secret proxy | Placeholder tokens via secret proxy |
| VM isolation | Yes | Yes |
| Resource limits | Yes | Yes (tighter defaults) |

### 8.3 Isolation Layers (All Containers)

These protections apply to both dev and deploy containers:

| Layer | Mechanism | Protects Against |
|---|---|---|
| **VM isolation** | All containers run inside Lima's VZ-backed Linux VM, not on the host kernel. | Kernel exploits, container escapes. |
| **Network isolation** | Per-project network namespaces. No cross-project communication. | Lateral movement, internal network scanning. |
| **Secret isolation** | Secrets are never in the container. Proxy substitution only for pre-approved domains. | Secret exfiltration, credential theft. |
| **Resource limits** | CPU, memory, disk, and network bandwidth caps per container. | DoS against the host, cryptomining, resource exhaustion. |
| **Host mount controls** | Host directories are only accessible when explicitly requested by Claude Code and approved by the user. Default is read-only. Dev containers only. | Unauthorized host filesystem access. |
| **Audit logging** | All MCP calls, resource changes, network connections, and secret substitutions are logged. | Post-incident investigation, anomaly detection. |

### 8.4 Deploy Container Hardening

When a project is deployed, the deploy engine builds a hardened image:

1. **Strip dev tooling.** Claude Code, MCP server, git, build tools, package managers, and compilers are excluded from the image.
2. **Remove shell access.** Where the runtime allows it (e.g., distroless base images for Node/Python), the shell is removed entirely.
3. **Read-only root filesystem.** The app's code and dependencies are on a read-only layer. Only explicitly declared data volumes are writable.
4. **Minimal egress.** The egress allowlist is reduced to only the domains the app actually needs (as declared during development). Package registries, CDNs, and other dev-time domains are removed.
5. **No MCP, no Claude Code.** The deploy container has no way to request new resources or modify its own configuration. It runs exactly what was built.
6. **Tighter resource limits.** Deploy containers get lower default CPU/memory limits than dev containers, tunable by the user.

### 8.5 Egress Allowlist Defaults

**Dev containers** have a broad default allowlist covering package registries, source code hosts, CDN assets, and the Terrarium internal services (MCP server, secret proxy). Claude Code can request additional domains via MCP, and the user can manage the allowlist in project settings.

**Deploy containers** start with an empty egress allowlist. During the deploy build, the allowlist is populated only with domains the app actually communicated with during development (as recorded in the audit log), plus any domains explicitly declared by Claude Code. The user can review and modify this list before deploying.

### 8.6 Secret Management Flow

```
1. Claude Code calls terrarium.secrets.request("OPENAI_API_KEY")
2. Desktop app prompts user: "Your app needs an OpenAI API key. Please enter it."
3. User enters the key. It is stored in the macOS Keychain, associated with the project.
4. MCP returns a placeholder token: __TERRARIUM_SECRET_OPENAI_API_KEY__
5. Claude Code configures the app to use the placeholder in API calls.
6. At runtime, the secret proxy intercepts outbound requests to api.openai.com
   and substitutes the placeholder with the real key.
7. The app never sees the real key.
```

---

## 9. Desktop App UI

The desktop app is intentionally simple — a single-view project dashboard. Users interact with Claude Code via their own Claude Code installation (Desktop app or CLI), not through Terrarium's UI.

### 9.1 Main Dashboard

The primary (and only) view is a grid of project cards. Each card shows:

- Project name and status indicator (running, stopped, creating, error).
- Workspace path (`~/Terrarium/<name>/`).
- Creation date.
- **Open Terminal** button — opens Terminal.app at the project workspace directory.
- **Delete** button — removes the container, namespace, and workspace directory.

Above the project grid:

- **VM Status Bar** — shows Lima VM status, version, and start/stop controls.
- **+ New Project** button — opens a dialog to name and create a new project.

### 9.2 Future Enhancements

As the project matures, the dashboard could grow to include:

- Quick action buttons (as configured by Claude Code via MCP).
- Resource summary (containers, ports, memory usage).
- Deploy status if a pinned/remote/cloud deploy exists.
- Project detail view with logs, resources, deploys, metrics, settings, and audit log tabs.

### 9.3 Global Settings *(Future)*

- Container runtime configuration (memory allocation to the VM, disk limits).
- Reverse proxy settings (TLS CA management, port configuration).
- Remote targets (add/remove remote Terrarium instances).
- Cloud provider credentials.
- Default egress allowlist.

---

## 10. Development Roadmap

### Phase 1 — Foundation (MVP)

- macOS desktop app (Tauri) with project dashboard.
- Container runtime integration (Lima + containerd/nerdctl via Apple VZ).
- Dev container provisioning with workspace bind-mount via virtiofs.
- Hooks-based command proxying (`PreToolUse` hook routes Bash into container).
- MCP server running on host with core resource tools (env info, allocate ports).
- Project workspace setup (`~/Terrarium/<name>/` with `.claude/settings.json`, `.mcp.json`, `.terrarium/config.json`).
- "Open Terminal" button to launch Claude Code in the project directory.
- Basic project lifecycle (create, start, stop, delete).

### Phase 2 — Developer Experience

- Reverse proxy with `*.terrarium.local` and local TLS.
- Log sink system with real-time UI viewer.
- UI action buttons (Claude Code → desktop app).
- Status indicators and process watching.
- Host mount requests with user approval flow.
- Cron job support.

### Phase 3 — Security

- Secret proxy with placeholder substitution.
- Egress filtering with allowlist management.
- Read-only filesystem layers for app code.
- Resource limits (CPU, memory, disk, network).
- Audit logging.

### Phase 4 — Deployment

- Local deployment ("pin" a version).
- Auto-start on launch for pinned deploys.
- Deploy-specific hostnames and independent log streams.
- Metrics collection and dashboard.

### Phase 5 — Distribution

- Remote deployment to other Terrarium instances.
- Remote instance discovery (Bonjour) and manual registration.
- Remote monitoring from the originating desktop app.
- Cloud deployment (Fly.io or Railway as first target).

### Phase 6 — Extras

- Tailscale Funnel integration.
- Additional cloud providers (AWS, GCP).
- Project templates (common app stacks pre-configured).
- Project sharing/export (share a project manifest + image with a friend).
- Multi-user support for remote instances.

---

## 11. Key Decisions

| Decision | Outcome |
|---|---|
| **Desktop app** | Tauri 2 (Rust backend + React/TypeScript frontend). Open source, cross-platform potential. |
| **Container runtime** | Lima with containerd/nerdctl on Apple's Virtualization.framework. Shared VM with prefixed container names in the default namespace. |
| **Claude Code integration** | Runs on the host (Desktop app or CLI). Bash commands proxied into container via `PreToolUse` hooks. File operations work directly on shared filesystem via virtiofs. |
| **MCP server location** | Runs on the **host** as a stdio subprocess of Claude Code (configured in `.mcp.json`). Communicates with Terrarium desktop app via host API (HTTP port 7778). |
| **Security model** | Two profiles: dev containers are permissive but localhost/LAN only; deploy containers are hardened and can face the internet. |
| **Workspace location** | `~/Terrarium/<project-name>/` — user-friendly, visible in Finder, shared with container via virtiofs bind-mount. |
| **Command proxying** | `PreToolUse` hooks are deterministic and transparent, unlike CLAUDE.md instructions which LLMs don't always follow. Every Bash command is guaranteed to be proxied. |
| **Base image** | Minimal (git, curl, build essentials, Node.js, Python). |
| **Data persistence** | Deploys start with empty volumes. Bootstrap steps run once on first deploy. Snapshot/restore as a future enhancement. |
| **Business model** | Open source. Terrarium Cloud as a future hosted deployment service. |

## 12. Implementation Notes

**Lima VM configuration.** The Lima VM (`lima-terrarium.yaml`) uses Apple's Virtualization.framework (VZ) with virtiofs mounts. The home directory is mounted read-only, with `~/Terrarium` mounted writable. This requires VM recreation if the mount configuration changes.

**Command proxying latency.** The `limactl shell → nerdctl exec` chain adds ~1-2s overhead per Bash command. This is acceptable for development but noticeable for quick operations like `ls`. The temp file approach (writing commands to `/tmp/terrarium-cmd-*` and piping via stdin) avoids shell quoting issues that arise from SSH argument re-tokenization.

**MCP host communication.** The MCP server runs on the host as a stdio subprocess of Claude Code. It communicates with the Terrarium desktop app via HTTP on port 7778 (the host API). This is simple and reliable — no need for Unix sockets, vsock, or other IPC mechanisms through the VM boundary.

**Project isolation.** Each project gets its own container in the default containerd namespace, prefixed with `terrarium-<project-id>-dev`. We avoid per-project namespaces because copying the dev base image into each namespace is expensive. Container and volume names are globally unique via the project UUID.

**Lima performance benchmarking.** Before committing to specific resource defaults, benchmark Lima's VZ backend on Apple Silicon for: cold VM start time (target: under 5 seconds), per-container start time (target: under 2 seconds), idle memory footprint, and virtio-fs filesystem throughput.
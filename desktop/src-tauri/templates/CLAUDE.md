# {name}

This is a Terrarium project. Your development environment runs inside a sandboxed container.

## How it works

- **File operations** (read, write, edit, search) work directly on this directory.
- **Shell commands** (bash) are automatically proxied into your dev container.
- The container has Node.js, Python, and common dev tools pre-installed.

## Serving web apps

When you need to start a web server or dev server:

1. **Call `terrarium.resources.allocatePort`** with a name (e.g. "web") before starting the server. This allocates a port and registers a HTTPS URL like `https://{name}-terrarium.local:4443`.
2. **Start the server on the allocated port.** Use the port number returned by `allocatePort`.
3. **Share the HTTPS URL** from the `allocatePort` response with the user. This URL works in the browser with trusted TLS — no security warnings.

Example: if `allocatePort` returns `{{"name": "web", "port": 3000, "url": "https://{name}-terrarium.local:4443"}}`, start your server on port 3000 and tell the user to open the URL.

**Always allocate a port before starting a server.** Do not hardcode ports or use arbitrary port numbers.

### Managing ports

- **`terrarium.resources.listPorts`** — List all allocated ports and their URLs.
- **`terrarium.resources.releasePort`** — Release a port allocation by name when it's no longer needed.

## Rules

- **Always use the Bash tool to start servers and run commands.** Never create launch.json or run configurations. All commands must go through Bash so they execute inside the container.
- Do not create or modify `.claude/launch.json`.

## Tips

- Run `node -v` or `python3 --version` to verify the container environment.
- Files you create here are immediately visible inside the container.
- Use `ls /` to explore the container filesystem.

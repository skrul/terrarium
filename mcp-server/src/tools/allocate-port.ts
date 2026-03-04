import { readState, writeState } from "../state.js";

const PORT_MIN = 3000;
const PORT_MAX = 9999;

interface AllocatePortResult {
  name: string;
  port: number;
  url: string;
}

export async function allocatePort(
  name: string,
  preferredPort?: number,
): Promise<AllocatePortResult> {
  const state = await readState();

  // If this name already has a port, return it
  if (state.ports[name] !== undefined) {
    const port = state.ports[name];
    return { name, port, url: `http://localhost:${port}` };
  }

  let port: number;

  if (preferredPort !== undefined) {
    // Validate preferred port is in range
    if (preferredPort < PORT_MIN || preferredPort > PORT_MAX) {
      throw new Error(
        `Port ${preferredPort} is out of range (${PORT_MIN}-${PORT_MAX})`,
      );
    }

    // Check if preferred port is already allocated
    const usedPorts = new Set(Object.values(state.ports));
    if (usedPorts.has(preferredPort)) {
      throw new Error(
        `Port ${preferredPort} is already allocated to "${Object.entries(state.ports).find(([, p]) => p === preferredPort)?.[0]}"`,
      );
    }

    port = preferredPort;
  } else {
    // Find the lowest available port
    const usedPorts = new Set(Object.values(state.ports));
    let found = false;
    port = PORT_MIN;

    for (let p = PORT_MIN; p <= PORT_MAX; p++) {
      if (!usedPorts.has(p)) {
        port = p;
        found = true;
        break;
      }
    }

    if (!found) {
      throw new Error("No available ports in range");
    }
  }

  // Persist the allocation
  state.ports[name] = port;
  await writeState(state);

  return { name, port, url: `http://localhost:${port}` };
}

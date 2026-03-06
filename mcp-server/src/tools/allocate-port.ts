import { readState, writeState } from "../state.js";

const PORT_MIN = 3000;
const PORT_MAX = 9999;

interface AllocatePortResult {
  name: string;
  port: number;
  url: string;
}

interface RouteResponse {
  hostname: string;
  url: string;
}

async function registerRoute(
  projectName: string,
  serviceName: string,
  port: number,
): Promise<RouteResponse | null> {
  const hostApi = process.env.TERRARIUM_HOST_API;
  if (!hostApi) return null;

  try {
    const resp = await fetch(`${hostApi}/routes`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        project_name: projectName,
        service_name: serviceName,
        port,
      }),
    });

    if (resp.ok) {
      return (await resp.json()) as RouteResponse;
    }

    console.error(`Route registration failed: ${resp.status} ${await resp.text()}`);
    return null;
  } catch (e) {
    console.error(`Route registration error: ${e}`);
    return null;
  }
}

export async function allocatePort(
  name: string,
  preferredPort?: number,
): Promise<AllocatePortResult> {
  const state = await readState();

  // If this name already has a port, return it
  if (state.ports[name] !== undefined) {
    const port = state.ports[name];
    const result: AllocatePortResult = { name, port, url: `http://localhost:${port}` };

    // Try to register route and upgrade URL
    const projectName = process.env.TERRARIUM_PROJECT_NAME;
    if (projectName) {
      const route = await registerRoute(projectName, name, port);
      if (route) {
        result.url = route.url;
      }
    }

    return result;
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

  const result: AllocatePortResult = { name, port, url: `http://localhost:${port}` };

  // Try to register route and upgrade URL
  const projectName = process.env.TERRARIUM_PROJECT_NAME;
  if (projectName) {
    const route = await registerRoute(projectName, name, port);
    if (route) {
      result.url = route.url;
    }
  }

  return result;
}

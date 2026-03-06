import { readState, writeState } from "../state.js";

interface ReleasePortResult {
  name: string;
  released: boolean;
}

async function unregisterRoute(
  projectName: string,
  serviceName: string,
): Promise<void> {
  const hostApi = process.env.TERRARIUM_HOST_API;
  if (!hostApi) return;

  try {
    const resp = await fetch(`${hostApi}/routes`, {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        project_name: projectName,
        service_name: serviceName,
      }),
    });

    if (!resp.ok) {
      console.error(`Route unregistration failed: ${resp.status} ${await resp.text()}`);
    }
  } catch (e) {
    console.error(`Route unregistration error: ${e}`);
  }
}

export async function releasePort(name: string): Promise<ReleasePortResult> {
  const state = await readState();

  if (state.ports[name] === undefined) {
    throw new Error(`No port allocation found for "${name}"`);
  }

  delete state.ports[name];
  await writeState(state);

  const projectName = process.env.TERRARIUM_PROJECT_NAME;
  if (projectName) {
    await unregisterRoute(projectName, name);
  }

  return { name, released: true };
}

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";

function getStatePath(): string {
  const workspace = process.env.TERRARIUM_WORKSPACE;
  if (workspace) {
    return join(workspace, ".terrarium", "state.json");
  }
  // Fallback for development/testing
  return "/tmp/terrarium-state.json";
}

export interface TerrariumState {
  ports: Record<string, number>;
}

function defaultState(): TerrariumState {
  return { ports: {} };
}

export async function readState(): Promise<TerrariumState> {
  try {
    const data = await readFile(getStatePath(), "utf-8");
    return JSON.parse(data) as TerrariumState;
  } catch {
    return defaultState();
  }
}

export async function writeState(state: TerrariumState): Promise<void> {
  const statePath = getStatePath();
  await mkdir(dirname(statePath), { recursive: true });
  await writeFile(statePath, JSON.stringify(state, null, 2), "utf-8");
}

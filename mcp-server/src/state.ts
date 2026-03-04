import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname } from "node:path";

const STATE_PATH = "/etc/terrarium/state.json";

export interface TerrariumState {
  ports: Record<string, number>;
}

function defaultState(): TerrariumState {
  return { ports: {} };
}

export async function readState(): Promise<TerrariumState> {
  try {
    const data = await readFile(STATE_PATH, "utf-8");
    return JSON.parse(data) as TerrariumState;
  } catch {
    return defaultState();
  }
}

export async function writeState(state: TerrariumState): Promise<void> {
  await mkdir(dirname(STATE_PATH), { recursive: true });
  await writeFile(STATE_PATH, JSON.stringify(state, null, 2), "utf-8");
}

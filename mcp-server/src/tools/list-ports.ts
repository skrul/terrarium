import { readState } from "../state.js";

interface PortEntry {
  name: string;
  port: number;
  url: string;
}

export async function listPorts(): Promise<PortEntry[]> {
  const state = await readState();
  const projectName = process.env.TERRARIUM_PROJECT_NAME;

  return Object.entries(state.ports).map(([name, port]) => {
    let url = `http://localhost:${port}`;
    if (projectName) {
      url = `https://${projectName}-terrarium.local:4443`;
    }
    return { name, port, url };
  });
}

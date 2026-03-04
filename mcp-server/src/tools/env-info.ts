import { readFile } from "node:fs/promises";
import { cpus, totalmem } from "node:os";
import { existsSync } from "node:fs";
import { readState } from "../state.js";

interface EnvInfo {
  os: string;
  user: string;
  workspace: string;
  workspaceExists: boolean;
  nodeVersion: string;
  memoryMB: number;
  cpuCount: number;
  allocatedPorts: Record<string, number>;
}

async function getOsInfo(): Promise<string> {
  try {
    const content = await readFile("/etc/os-release", "utf-8");
    const prettyName = content
      .split("\n")
      .find((line) => line.startsWith("PRETTY_NAME="));
    if (prettyName) {
      return prettyName.split("=")[1].replace(/"/g, "");
    }
  } catch {
    // fall through
  }
  return "Linux (unknown)";
}

export async function getEnvInfo(): Promise<EnvInfo> {
  const workspace = "/home/terrarium/workspace";
  const state = await readState();

  return {
    os: await getOsInfo(),
    user: process.env.USER ?? "terrarium",
    workspace,
    workspaceExists: existsSync(workspace),
    nodeVersion: process.version,
    memoryMB: Math.round(totalmem() / (1024 * 1024)),
    cpuCount: cpus().length,
    allocatedPorts: state.ports,
  };
}

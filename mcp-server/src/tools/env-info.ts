import { cpus, totalmem, platform, arch } from "node:os";
import { existsSync } from "node:fs";
import { readState } from "../state.js";

interface EnvInfo {
  os: string;
  arch: string;
  user: string;
  workspace: string;
  workspaceExists: boolean;
  containerName: string;
  nodeVersion: string;
  memoryMB: number;
  cpuCount: number;
  allocatedPorts: Record<string, number>;
}

export async function getEnvInfo(): Promise<EnvInfo> {
  const workspace = process.env.TERRARIUM_WORKSPACE ?? "";
  const containerName = process.env.TERRARIUM_CONTAINER_NAME ?? "";
  const state = await readState();

  return {
    os: platform(),
    arch: arch(),
    user: process.env.USER ?? "unknown",
    workspace,
    workspaceExists: workspace !== "" && existsSync(workspace),
    containerName,
    nodeVersion: process.version,
    memoryMB: Math.round(totalmem() / (1024 * 1024)),
    cpuCount: cpus().length,
    allocatedPorts: state.ports,
  };
}

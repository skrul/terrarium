import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { getEnvInfo } from "./tools/env-info.js";
import { allocatePort } from "./tools/allocate-port.js";
import { listPorts } from "./tools/list-ports.js";
import { releasePort } from "./tools/release-port.js";

const server = new McpServer({
  name: "terrarium",
  version: "0.1.0",
});

// terrarium.env.info — returns container environment information
server.tool(
  "terrarium.env.info",
  "Get current environment info: OS, node version, memory, CPUs, workspace path, and allocated ports",
  {},
  async () => {
    const info = await getEnvInfo();
    return {
      content: [{ type: "text", text: JSON.stringify(info, null, 2) }],
    };
  },
);

// terrarium.resources.allocatePort — allocate a named port
server.tool(
  "terrarium.resources.allocatePort",
  "Allocate a named port for your application. Returns the port number and localhost URL. If the name already has a port, returns the existing allocation.",
  {
    name: z.string().describe("A descriptive name for this port allocation (e.g. 'web', 'api', 'dev-server')"),
    port: z.number().int().min(3000).max(9999).optional().describe("Preferred port number (3000-9999). If omitted, the lowest available port is assigned."),
  },
  async ({ name, port }) => {
    try {
      const result = await allocatePort(name, port);
      return {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
      };
    } catch (err) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: err instanceof Error ? err.message : String(err),
          },
        ],
      };
    }
  },
);

// terrarium.resources.listPorts — list all allocated ports
server.tool(
  "terrarium.resources.listPorts",
  "List all allocated ports and their URLs",
  {},
  async () => {
    try {
      const result = await listPorts();
      return {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
      };
    } catch (err) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: err instanceof Error ? err.message : String(err),
          },
        ],
      };
    }
  },
);

// terrarium.resources.releasePort — release a named port allocation
server.tool(
  "terrarium.resources.releasePort",
  "Release a previously allocated port by name. Removes the allocation and unregisters the proxy route.",
  {
    name: z.string().describe("Name of the port allocation to release"),
  },
  async ({ name }) => {
    try {
      const result = await releasePort(name);
      return {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
      };
    } catch (err) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: err instanceof Error ? err.message : String(err),
          },
        ],
      };
    }
  },
);

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});

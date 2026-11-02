import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";

export interface FixtureResource {
  uri: string;
  name: string;
  title?: string;
  mimeType?: string;
  text: string;
}

export function serve(resources: FixtureResource[]): { server: McpServer; set(resources: FixtureResource[]): void };

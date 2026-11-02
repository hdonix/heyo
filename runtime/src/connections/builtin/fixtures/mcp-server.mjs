// An MCP server for tests, built on the SDK's own: its resources are what
// a JSON file says, `{ "resources": [{ "uri", "name", "title"?, "mimeType"?,
// "text" }…] }`, and it has one tool, `echo`, which it never expects to be
// called. Run as `node mcp-server.mjs <file>`, it speaks over its input and
// output, writes its pid to `<file>.pid`, and tells its client when the
// file changes; `serve(source)` gives one to connect to in the same process.

import { readFileSync, watch, writeFileSync } from "node:fs";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";

/// A server of `resources`; `set` puts others in their place.
export function serve(resources) {
  const server = new McpServer({ name: "heyo-test", version: "1.0.0" }, { capabilities: { resources: { listChanged: true }, tools: {} } });
  server.registerTool("echo", { title: "Echo", description: "Says it back" }, async () => ({ content: [{ type: "text", text: "echo" }] }));
  let registered = [];
  const set = (all) => {
    for (const resource of registered) resource.remove();
    registered = all.map(({ uri, name, title, mimeType, text }) =>
      server.registerResource(name, uri, { title, mimeType }, async (asked) => ({ contents: [{ uri: asked.href, mimeType, text }] })),
    );
  };
  set(resources);
  return { server, set };
}

if (process.argv[2]) {
  const file = process.argv[2];
  const read = () => JSON.parse(readFileSync(file, "utf8")).resources;
  const { server, set } = serve(read());
  writeFileSync(`${file}.pid`, String(process.pid));
  watch(file, () => {
    try {
      set(read());
    } catch {
      // Written halfway: the next change has it whole.
    }
  });
  await server.connect(new StdioServerTransport());
}

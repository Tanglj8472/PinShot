import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const serverEntry = path.resolve(here, "../src/index.ts");

function textFromToolResult(result: unknown): string {
  if (typeof result !== "object" || result === null || !("content" in result)) {
    throw new Error(`unexpected tool result: ${JSON.stringify(result)}`);
  }

  const content = (result as { content: unknown }).content;
  if (!Array.isArray(content)) {
    throw new Error(`tool result.content is not an array: ${JSON.stringify(result)}`);
  }

  const texts = content
    .filter(
      (item): item is { type: "text"; text: string } =>
        typeof item === "object" &&
        item !== null &&
        "type" in item &&
        item.type === "text" &&
        "text" in item &&
        typeof item.text === "string",
    )
    .map((item) => item.text);

  if (texts.length === 0) {
    throw new Error(`no text content in tool result: ${JSON.stringify(result)}`);
  }

  return texts.join("");
}

async function main(): Promise<void> {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ["--import", "tsx", serverEntry],
    stderr: "pipe",
  });

  const client = new Client({
    name: "pinshot-mcp-smoke",
    version: "0.1.0",
  });

  await client.connect(transport);

  const listed = await client.listTools();
  const names = listed.tools.map((tool) => tool.name);
  if (names.length !== 1 || names[0] !== "ping") {
    throw new Error(`expected only ping, got: ${names.join(",") || "(empty)"}`);
  }

  const defaultResult = await client.callTool({
    name: "ping",
    arguments: {},
  });
  const defaultText = textFromToolResult(defaultResult);
  if (defaultText !== "pong") {
    throw new Error(`default ping expected "pong", got ${JSON.stringify(defaultText)}`);
  }

  const customResult = await client.callTool({
    name: "ping",
    arguments: { message: "hello-夕下" },
  });
  const customText = textFromToolResult(customResult);
  if (customText !== "hello-夕下") {
    throw new Error(
      `custom ping expected "hello-夕下", got ${JSON.stringify(customText)}`,
    );
  }

  await client.close();

  console.error("smoke ok");
  console.error(`tools: ${names.join(",")}`);
  console.error(`ping(): ${defaultText}`);
  console.error(`ping({ message: "hello-夕下" }): ${customText}`);
}

main().catch((error: unknown) => {
  const text =
    error instanceof Error ? (error.stack ?? error.message) : String(error);
  console.error(text);
  process.exit(1);
});

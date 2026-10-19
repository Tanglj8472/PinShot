import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { captureScreen } from "./capture.js";

const SERVER_NAME = "pinshot-mcp";
const SERVER_VERSION = "0.1.0";
const DEFAULT_PONG = "pong";

/**
 * 组装最小 MCP Server。
 * 只注册 ping：可选 message，默认 pong，返回纯文本。
 */
export function createServer(): McpServer {
  const server = new McpServer({
    name: SERVER_NAME,
    version: SERVER_VERSION,
  });

  server.registerTool(
    "ping",
    {
      title: "Ping",
      description: "连通性检查。返回 message 文本；省略时返回 pong。",
      inputSchema: {
        message: z
          .string()
          .optional()
          .default(DEFAULT_PONG)
          .describe("要返回的文本，默认 pong"),
      },
    },
    async ({ message }) => ({
      content: [{ type: "text", text: message }],
    }),
  );

  server.registerTool(
    "capture_screen",
    {
      title: "截取屏幕",
      description: "立即截取 Windows 全部显示器的当前画面，返回 PNG 图片。无需手动框选。",
      inputSchema: {},
    },
    async () => {
      try {
        return { content: [{ type: "image" as const, mimeType: "image/png", data: await captureScreen() }] };
      } catch (error) {
        return {
          isError: true,
          content: [{ type: "text" as const, text: error instanceof Error ? error.message : String(error) }],
        };
      }
    },
  );

  return server;
}

async function main(): Promise<void> {
  const server = createServer();
  const transport = new StdioServerTransport();
  await server.connect(transport);
  // stdout 是 JSON-RPC 通道，只能往 stderr 打日志。
  console.error(`${SERVER_NAME} ${SERVER_VERSION} 已通过 stdio 启动`);
}

main().catch((error: unknown) => {
  const text =
    error instanceof Error ? (error.stack ?? error.message) : String(error);
  console.error(text);
  process.exit(1);
});

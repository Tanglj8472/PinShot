import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { requestNative, preview } from "./automation.js";

const SERVER_NAME = "pinshot-mcp";
const SERVER_VERSION = "0.2.0";
const DEFAULT_PONG = "pong";

/**
 * 组装最小 MCP Server。
 * Native PinShot automation tools over a local named pipe.
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
        const result = await requestNative("capture");
        try {
          return { content: [
            { type: "text" as const, text: JSON.stringify(result) },
            { type: "image" as const, mimeType: "image/png", data: await preview(Number(result.session_id)) },
          ] };
        } catch (error) {
          await requestNative("close", { session_id: result.session_id }).catch(() => {});
          throw error;
        }
      } catch (error) {
        return {
          isError: true,
          content: [{ type: "text" as const, text: error instanceof Error ? error.message : String(error) }],
        };
      }
    },
  );

  const session = z.number().int().positive().max(Number.MAX_SAFE_INTEGER);
  const point = z.object({ x: z.number().finite().nonnegative(), y: z.number().finite().nonnegative() });
  const guarded = async (action: () => Promise<Record<string, unknown>>) => {
    try { return { content: [{ type: "text" as const, text: JSON.stringify(await action()) }] }; }
    catch (error) { return { isError: true, content: [{ type: "text" as const, text: error instanceof Error ? error.message : String(error) }] }; }
  };
  server.registerTool("get_state", {
    description: "查询运行中的 PinShot、截图会话、图片尺寸、桌面原点和标注 ID。标注坐标为图片左上角起的物理像素。",
    inputSchema: {},
  }, async () => guarded(() => requestNative("status")));
  server.registerTool("add_annotation", {
    description: "向截图会话添加原生标注。text 使用一个点，其余使用两个点；width 为线宽或字号，strength 为模糊/马赛克程度。返回 annotation_id。",
    inputSchema: {
      session_id: session,
      kind: z.enum(["rect", "ellipse", "line", "arrow", "text", "blur", "mosaic"]),
      points: z.array(point).min(1).max(2),
      text: z.string().max(4096).optional(),
      width: z.number().int().min(1).max(200).optional(),
      strength: z.number().int().min(1).max(30).optional(),
      color: z.object({ r: z.number().int().min(0).max(255), g: z.number().int().min(0).max(255), b: z.number().int().min(0).max(255), a: z.number().int().min(0).max(255).default(255) }).optional(),
    },
  }, async ({ session_id, ...annotation }) => guarded(() => requestNative("annotate", { session_id, annotation })));
  server.registerTool("remove_annotation", {
    description: "删除指定会话中的指定标注。", inputSchema: { session_id: session, annotation_id: session },
  }, async (args) => guarded(() => requestNative("remove", args)));
  server.registerTool("preview_session", {
    description: "返回包含所有标注的原生 PNG 预览。", inputSchema: { session_id: session },
  }, async ({ session_id }) => {
    try { return { content: [{ type: "image" as const, mimeType: "image/png", data: await preview(session_id) }] }; }
    catch (error) { return { isError: true, content: [{ type: "text" as const, text: error instanceof Error ? error.message : String(error) }] }; }
  });
  server.registerTool("save_session", {
    description: "将带标注截图保存为 PNG，path 必须是绝对路径，父目录必须存在，已有文件不会覆盖。",
    inputSchema: { session_id: session, path: z.string().min(1) },
  }, async (args) => guarded(() => requestNative("save", args)));
  server.registerTool("close_session", {
    description: "关闭截图会话并释放内存；最多同时保留四个会话。", inputSchema: { session_id: session },
  }, async (args) => guarded(() => requestNative("close", args)));

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

import { connect } from "node:net";
import { randomUUID } from "node:crypto";
import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

export async function requestNative(op: string, args: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
  if (process.platform !== "win32") throw new Error("原生自动化接口当前仅支持 Windows。");
  const id = randomUUID();
  return new Promise((resolve, reject) => {
    const socket = connect("\\\\.\\pipe\\PinShotAutomation-v1");
    let received = "";
    let done = false;
    const timer = setTimeout(() => finish(new Error("PinShot 操作超时（30 秒）")), 30_000);
    function finish(error?: Error, result?: Record<string, unknown>) {
      if (done) return;
      done = true;
      clearTimeout(timer);
      socket.destroy();
      if (error) reject(error); else resolve(result ?? {});
    }
    socket.setEncoding("utf8");
    socket.on("connect", () => socket.write(JSON.stringify({ ...args, id, op }) + "\n"));
    socket.on("error", (error) => finish(new Error(`无法连接 PinShot。请先运行本分支构建的 pinshot.exe --automation。${error.message}`)));
    socket.on("end", () => { if (!done) finish(new Error("PinShot 未返回完整响应")); });
    socket.on("data", (chunk: string) => {
      received += chunk;
      if (Buffer.byteLength(received) > 4 * 1024 * 1024) { finish(new Error("PinShot 响应过大")); return; }
      const end = received.indexOf("\n");
      if (end < 0) return;
      try {
        const reply = JSON.parse(received.slice(0, end));
        if (reply.id !== id) throw new Error("PinShot 响应 ID 不匹配");
        if (reply.error) throw new Error(String(reply.error));
        finish(undefined, reply.result);
      } catch (error) { finish(error instanceof Error ? error : new Error(String(error))); }
    });
  });
}

export async function preview(sessionId: number): Promise<string> {
  const directory = await mkdtemp(path.join(tmpdir(), "pinshot-mcp-"));
  try {
    const output = path.join(directory, "preview.png");
    await requestNative("preview", { session_id: sessionId, path: output });
    if ((await stat(output)).size > 24 * 1024 * 1024) throw new Error("预览超过 24 MB");
    const bytes = await readFile(output);
    if (!bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) throw new Error("无效 PNG");
    return bytes.toString("base64");
  } finally { await rm(directory, { recursive: true, force: true }); }
}

import { execFile } from "node:child_process";
import { access, mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execute = promisify(execFile);
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

async function executable(): Promise<string> {
  const configured = process.env.PINSHOT_EXE;
  const filename = process.platform === "win32" ? "pinshot.exe" : "pinshot";
  const candidates = configured ? [path.resolve(configured)] : [
    path.join(repository, "rust", "target", "release", filename),
    path.join(repository, "rust", "target", "debug", filename),
  ];
  for (const candidate of candidates) {
    try { await access(candidate); return candidate; } catch { /* Try the next build. */ }
  }
  throw new Error("找不到 PinShot 原生程序。请先在 rust 目录运行 cargo build --release --bin pinshot，或用 PINSHOT_EXE 指定支持 --capture-to 的程序。");
}

/** Reuse PinShot's native capture backend and image encoder. */
export async function captureScreen(): Promise<string> {
  const binary = await executable();
  const directory = await mkdtemp(path.join(tmpdir(), "pinshot-mcp-"));
  const output = path.join(directory, "capture.png");
  try {
    await execute(binary, ["--capture-to", output], {
      windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024,
    });
    if ((await stat(output)).size > 24 * 1024 * 1024) {
      throw new Error("截图超过 MCP 图片大小限制（24 MB）。");
    }
    const bytes = await readFile(output);
    if (!bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
      throw new Error("PinShot 没有返回有效 PNG。");
    }
    return bytes.toString("base64");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const client = new Client({ name: "pinshot-workflow-test", version: "0.2.0" });
const transport = new StdioClientTransport({ command: process.execPath,
  args: [path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../dist/index.js")], stderr: "inherit" });
const directory = await mkdtemp(path.join(tmpdir(), "pinshot-workflow-"));
let session: number | undefined;
async function call(name: string, args: Record<string, unknown>) {
  const result = await client.callTool({ name, arguments: args });
  assert.ok(!result.isError, JSON.stringify(result));
  return result;
}
function text(result: Awaited<ReturnType<typeof call>>) {
  const item = (result.content as Array<{ type: string; text?: string }>).find((c) => c.type === "text");
  return JSON.parse(item?.text ?? "{}");
}
try {
  await client.connect(transport);
  const before = text(await call("get_state", {}));
  const captured = text(await call("capture_screen", {}));
  session = captured.session_id;
  assert.ok(session && captured.width > 40 && captured.height > 40);
  const annotation = text(await call("add_annotation", { session_id: session,
    kind: "rect", points: [{ x: 5, y: 5 }, { x: 35, y: 35 }], width: 3 }));
  const state = text(await call("get_state", {}));
  assert.equal(state.pid, before.pid, "All actions must use the same resident PinShot process");
  assert.equal(state.sessions.find((s: { session_id: number }) => s.session_id === session).annotations.length, 1);
  const preview = await call("preview_session", { session_id: session });
  assert.ok((preview.content as Array<{ type: string }>).some((c) => c.type === "image"));
  const output = path.join(directory, "result.png");
  await call("save_session", { session_id: session, path: output });
  const bytes = await readFile(output);
  assert.equal(bytes.readUInt32BE(0), 0x89504e47);
  const duplicate = await client.callTool({ name: "save_session", arguments: { session_id: session, path: output } });
  assert.equal(duplicate.isError, true, "Must reject overwriting an existing file");
  await call("remove_annotation", { session_id: session, annotation_id: annotation.annotation_id });
  await call("close_session", { session_id: session });
  session = undefined;
  console.error("workflow ok: native capture -> annotate -> preview -> save -> remove -> close");
} finally {
  if (session !== undefined) await client.callTool({ name: "close_session", arguments: { session_id: session } }).catch(() => {});
  await client.close();
  await rm(directory, { recursive: true, force: true });
}

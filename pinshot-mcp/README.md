# pinshot-mcp

PinShot 仓库的旁路最小 [MCP](https://modelcontextprotocol.io) Server，用来学习 stdio 传输和后续加工具。

它**不参与** `rust/` 里的截图主程序，也不改 Python 历史树。当前只暴露一个 tool：`ping`。

## 它做什么

| 项目 | 说明 |
| --- | --- |
| 传输 | MCP **stdio**（stdin 收 JSON-RPC，stdout 回响应） |
| Tool | `ping` |
| 参数 | `message`：可选字符串，默认 `pong` |
| 返回 | 一段文本。省略参数时是 `pong`，传入时原样返回 |

生命周期可以这样理解：

```
Cursor / Inspector 拉起进程
        ↓
initialize / initialized 握手
        ↓
tools/list  →  看到 ping
        ↓
tools/call ping  →  返回文本
        ↓
关闭 stdin（EOF）→  进程退出
```

日志只能打到 **stderr**。`console.log` 会污染 stdout，导致客户端解析失败。

## 环境

- Node.js 20 或更高（仓库文档按 20+ 写；本机用 `node -v` 确认）
- 不需要改 PinShot 主程序，也不需要 Rust 工具链

## 安装与启动

在仓库根目录执行：

```powershell
cd pinshot-mcp
npm install
npm start
```

`npm start` 等于 `npx tsx src/index.ts`。进程会挂在 stdio 上等待客户端，终端里通常只看到：

```text
pinshot-mcp 0.1.0 已通过 stdio 启动
```

这是正常的：没有客户端往 stdin 写协议消息时，它就一直等。用 `Ctrl+C` 结束。

Cursor / Inspector 的 `command` **不要**写成 `npm start`：npm 会往 stdout 打脚本横幅，污染 JSON-RPC。用下面的 `npx tsx` 或 `node dist/index.js`。

可选：编译成 JS 后再用 Node 启动（Cursor 配置更稳，不依赖 `tsx`）：

```powershell
npm run build
node dist/index.js
```

## 本地验收 ping

不要手写 JSON-RPC。用官方 SDK 客户端走完整生命周期：

```powershell
npm run smoke
```

成功时 stderr 类似：

```text
smoke ok
tools: ping
ping(): pong
ping({ message: "hello-夕下" }): hello-夕下
```

## 用 MCP Inspector 连上

Inspector 会自己拉起 server 并打开浏览器：

```powershell
npm run inspector
```

等价于：

```powershell
npx --yes @modelcontextprotocol/inspector tsx src/index.ts
```

在页面里点 Connect，打开 Tools，选 `ping`：

- 不填 `message` → 应返回 `pong`
- `message` 填 `hello` → 应返回 `hello`

## 在 Cursor 里配置

Cursor 读项目级 `.cursor/mcp.json` 或用户级 `~/.cursor/mcp.json`。stdio server 需要 `command` + `args`。

先在 `pinshot-mcp/` 里执行一次 `npm install`。然后把下面整段放进 MCP 配置（把路径换成你的仓库位置，或用 Cursor 的 `${workspaceFolder}`）。

### 开发态（tsx 直接跑 TypeScript）

```json
{
  "mcpServers": {
    "pinshot-mcp": {
      "command": "npx",
      "args": [
        "--yes",
        "tsx",
        "${workspaceFolder}/pinshot-mcp/src/index.ts"
      ]
    }
  }
}
```

Windows 用户级配置若不用变量，可以写成绝对路径：

```json
{
  "mcpServers": {
    "pinshot-mcp": {
      "command": "npx",
      "args": [
        "--yes",
        "tsx",
        "D:\\src\\pinshot\\pinshot-mcp\\src\\index.ts"
      ]
    }
  }
}
```

### 更稳的启动方式（先 build）

```powershell
cd pinshot-mcp
npm install
npm run build
```

```json
{
  "mcpServers": {
    "pinshot-mcp": {
      "command": "node",
      "args": ["${workspaceFolder}/pinshot-mcp/dist/index.js"]
    }
  }
}
```

保存配置后重载 Cursor 窗口。Agent / MCP 面板里应出现 `pinshot-mcp`，并可调用 `ping`。

若 Cursor 提示需要显式写传输类型，补上 `"type": "stdio"` 即可。

## 目录结构

```text
pinshot-mcp/
├── package.json          # 依赖与 npm scripts
├── package-lock.json     # 锁定安装版本
├── tsconfig.json
├── README.md
├── src/
│   └── index.ts          # stdio server + ping
└── scripts/
    └── smoke.ts          # 拉起 server，listTools + 两次 callTool
```

后续加截图、贴图之类的 tool 时：继续改 `src/index.ts` 的 `createServer()`，不要把逻辑塞进 `rust/crates/pinshot`。主程序保持独立；MCP 只作为旁路进程。

## 依赖说明

| 包 | 用途 |
| --- | --- |
| `@modelcontextprotocol/sdk` | 官方 TypeScript SDK（v1 维护线，stdio + Cursor / Inspector 兼容最好） |
| `zod` | `ping` 参数校验（SDK 的 peer dependency） |
| `tsx` | 开发态直接运行 `.ts` |
| `typescript` | `npm run build` |

当前选 v1 而不是 v2 的 `@modelcontextprotocol/server`，是因为 Cursor 与 MCP Inspector 对 v1 stdio 最稳。以后要跟官方 v2 教程对齐，可以再单独升级，不必动截图主程序。

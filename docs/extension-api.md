# Heyo 扩展 API 规格（草案 v0.3）

状态：草案，已按阶段 A–E 的实现修订。日期：2026-10-11。

本文定义 `@heyo/api`。它是 `@raycast/api` 的**超集**：Raycast 接口的每一个导出都以
同样的名字和签名存在，所以迁移一个扩展只需要把 `import … from "@raycast/api"` 换成
`import … from "@heyo/api"`。在此之上，它增加 Heyo 自有的能力（窗口、屏幕与文字识别、
听写、本地记忆、文件索引、系统操作、进行中的活动、根搜索结果）。

不改 import 的 Raycast 扩展照常运行；两个包名在 Heyo 里指向同一份实现。

v0.2 相对 v0.1 的变化：`@heyo/api` 从"只含新增能力"改为"Raycast 接口的超集"；
因此重写了第 1、3、8 节，第 7 节里与 Raycast 重名的模块改为在原接口上扩展。

v0.3 相对 v0.2 的变化：阶段 A–E 做完之后（第 11 节），把规格改到与实现一致，改过的地方标"改"。
内置命令改用这套接口时暴露出的缺口都补进了接口：`MemoryItem` 多了图像尺寸、来源应用的名称和图标、
首次复制时间与次数、"按语义找到"的标记，其中只有剪贴板来源才有的字段改为可选；`Memory.search`
可按语义查找，上限改为 300；`ClipboardHistory.search` 去掉，读取一律走 `Memory.search`；
`Files.search` 返回 `total`；`System` 加音频设备；写明 `Windows.list()` 的顺序约定；`AI.ask`
带对话历史（`messages`）、文本材料（`material`）和整段回调（`onText`），Heyo 自己的 Ask AI、AI Chat
和选中文字操作改走它，失败一律是 `HeyoError`。与实现对齐的还有：权限的拒绝只记 60 秒、没有
"仅这一次"；`offline` 恒为假；`apiVersion` 不拦安装。第 12 节的问题 5 和 7 已由实现回答。
此前漏写的 `LiveActivity` 模块（7.9）和 `activity` 权限（5.1）也补上了；它的设计在 `docs/live-activities.md`。

文末第 12 节列出需要你拍板的问题；它们会影响实现，但不影响整体结构。

---

## 1. 目标与非目标

**目标**

1. **换一个 import 就能迁移**。`@raycast/api` 的每个导出在 `@heyo/api` 里同名、同签名、同行为。
   做不到同行为的地方（macOS 专有的概念）有明确的、有文档的降级，而不是缺失或崩溃。
2. 第三方扩展能做到今天只有内置命令能做的事。内置命令改为使用同一套接口，不再有私有通道。
3. 迁移后的扩展仍然可以在 Raycast 里运行：在 Heyo 上多出能力，在 Raycast 上正常降级。
4. 每项能力在当前会话（X11、GNOME Wayland、Windows、其他）是否可用，扩展能在调用前查到。
5. 扩展读取 Heyo 保管的用户数据（剪贴板历史、记忆、屏幕内容、麦克风）之前，用户明确同意过。
6. 接口可以演进而不破坏已发布的扩展。

**非目标**

- 不承诺"换 import 之后任何扩展都能用"。接口兼容解决的是接口；自己去调 macOS 程序、
  AppleScript、`/Applications` 路径的扩展仍然需要移植。`heyo ext check` 负责把这类问题找出来（第 9 节）。
- 不提供 D-Bus、systemd、注册表的封装。扩展在 Node.js 里可以直接用现成的库或命令行。
- v1 不做沙箱（见 5.4）。
- 不让扩展注册全局热键。热键由用户在设置里绑定到命令。

---

## 2. 设计原则

- **按能力分模块，不按平台分**。扩展写 `Windows.list()`，不写 `X11.listWindows()`。
- **先问再做**。每个模块有 `supported()`，返回 `"full" | "limited" | "none"`，与主机的
  `Supported`（`src-tauri/src/platform/desktop.rs`）一一对应。
- **失败有类型**。所有错误是带 `code` 的 `HeyoError`，扩展不需要解析文案。
- **主机是唯一执行者**。扩展运行时只转发请求；窗口、截屏、索引都在主机里做，
  所以 Windows 与 Wayland 的差异不会泄漏到扩展里。
- **一处声明**。请求与返回类型在 `src-tauri/src/wire.rs` 声明，由 `cargo test`
  生成到 `shared/bindings/`，`@heyo/api` 的类型从那里来。
  改：Heyo 加的每个函数（所在模块、名字、它发的请求）在 `runtime/src/heyo/members.ts` 声明一次。
  它需要的权限由主机说了算（`permissions.rs` 的 `needed`，`cargo test` 写到 `shared/bindings/needs.ts`），
  回复的类型按请求记在 `shared/wire.ts` 的 `ExtReplies` 里。只是转发请求的函数直接由声明生成；
  类型包、Heyo 之外的桩、`heyo ext check` 认得的模块和权限、测试里的调用表，都拿这份声明对照，
  少一处多一处都过不了编译或测试。

---

## 3. 包、加载与共存

### 3.1 包

- 包名 `@heyo/api`，发布到 npm。
- 它导出两部分：
  1. **Raycast 部分**：`@raycast/api` 的全部导出，名字、签名、类型都相同（清单见第 8 节）。
  2. **Heyo 部分**：第 7 节定义的新增模块，以及对若干 Raycast 模块的扩展。
- 在 Heyo 里，运行时拦截模块加载（`runtime/src/runtime.ts` 的 `provided` 表），
  `@raycast/api` 和 `@heyo/api` 都换成**同一个对象**。这一点是必须的：扩展常常同时依赖
  `@raycast/utils`，后者内部引用 `@raycast/api`；两个名字指向同一份实现，导航栈、
  Toast、缓存才不会各有一份。

三种情况下的行为：

| 扩展怎么写 | 在 Heyo 里 | 在 Raycast 里 |
|---|---|---|
| 只用 `@raycast/api`（商店里的现状） | 照常运行，用不到新增能力 | 照常运行 |
| 改用 `@heyo/api`，只用 Raycast 部分 | 照常运行 | 照常运行（见下） |
| 改用 `@heyo/api`，用到 Heyo 部分 | 全部可用 | Raycast 部分正常；Heyo 部分降级 |

为了让第二、三种情况在 Raycast 里也能运行，npm 上的 `@heyo/api` 是这样构成的：

```ts
// npm 上的 @heyo/api（Heyo 之外才会真正加载到）
export * from "@raycast/api";          // peerDependency，由 Raycast 提供
export const isHeyo = false;
export const Windows = { supported: async () => "none", /* 其余抛 HeyoError("unsupported") */ };
// …其余 Heyo 模块同理
```

```ts
import { isHeyo, List, Windows } from "@heyo/api";

if (isHeyo && (await Windows.supported()) !== "none") {
  const windows = await Windows.list();
}
```

### 3.1.1 迁移一个扩展

1. `npm install @heyo/api`。
2. 把源码里的 `"@raycast/api"` 替换为 `"@heyo/api"`。`heyo migrate <目录>` 做这一步，
   并在 `package.json` 里加上 `heyo` 字段。
3. `@raycast/utils` 不需要改：它继续引用 `@raycast/api`，在 Heyo 里拿到的是同一份实现。
4. `heyo ext check` 报告接口之外的问题：调用了 macOS 程序、AppleScript、写死的 macOS 路径。
5. 用 `heyo build` 构建（它把 `@heyo/api`、`@raycast/api`、`react` 标为 external，产物与 `ray build` 相同）。

不做第 2 步的扩展不受影响；第 2 步的意义是拿到 Heyo 部分的类型，并声明权限。

### 3.1.2 类型从哪里来

Raycast 部分的类型有两种做法，需要你决定（第 12 节问题 7）：

- **转导出**：`@heyo/api` 把 `@raycast/api` 列为 peerDependency，类型直接 `export *`。
  开发者仍需安装 `@raycast/api`，但类型永远与 Raycast 一致，我们不需要维护。
- **自有类型**：我们独立写一份等价的类型声明。开发者只装一个包，但我们要跟着 Raycast 的版本更新，
  它的 `package.json` 标注的许可是 MIT；类型声明是否另有条款我没有查。

规格以"转导出"为默认。

### 3.2 清单

扩展在 `package.json` 里加一个 `heyo` 字段。没有这个字段的扩展不能使用需要权限的接口。

```json
{
  "name": "window-sessions",
  "commands": [{ "name": "restore", "title": "Restore Layout", "mode": "no-view" }],
  "heyo": {
    "apiVersion": 1,
    "permissions": ["windows", "memory.read"],
    "platforms": ["linux", "windows"]
  }
}
```

| 字段 | 含义 |
|---|---|
| `apiVersion` | 扩展编写时所依据的主版本。改：Heyo 支持的主版本低于它时**不拦安装**（安装走的是 Raycast 商店的流程，不读这个字段）；需要权限的调用以 `unsupported` 失败，说明"为更新的 Heyo 编写"。 |
| `permissions` | 扩展可能用到的权限（见第 5 节）。未列出的权限在调用时直接拒绝，不询问。改：安装时不显示这份清单，用到时才询问。 |
| `platforms` | 可选。不含当前平台时，扩展的命令不进入根搜索。 |

### 3.3 命令模式

沿用 `view`、`no-view`、`menu-bar`，新增一种：

- `provider`：向根搜索贡献结果（见 7.10）。

---

## 4. 版本与兼容

- `apiVersion` 是整数主版本。同一主版本内只增不改：新增函数、新增可选参数、新增返回字段。
- 删除或改变语义需要新主版本。Heyo 同时支持当前主版本和上一个主版本。
- 细粒度探测用 `has()`，不靠版本号比较：

```ts
import { has } from "@heyo/api";
if (has("Screen.recognizeText")) { /* … */ }
```

- `environment.heyoVersion` 给出 Heyo 的版本字符串，仅用于日志和问题报告。

---

## 5. 权限

### 5.1 权限表

| 权限 | 允许 | 默认 |
|---|---|---|
| `windows` | 列出、激活、关闭、移动其他应用的窗口 | 首次使用时询问 |
| `screen.capture` | 截取屏幕或窗口的图像 | 每次由用户框选时无需询问；整屏或指定窗口首次询问 |
| `screen.text` | 对图像做文字识别 | 不询问（只处理扩展自己给的图像） |
| `microphone` | 录音并转写 | 首次使用时询问 |
| `clipboard.history` | 读取剪贴板历史 | 首次使用时询问 |
| `memory.read` | 搜索本地记忆（跨来源） | 首次使用时询问 |
| `memory.write` | 向本地记忆写入扩展自己的条目 | 首次使用时询问 |
| `files.search` | 查询文件索引（只返回路径） | 不询问 |
| `system` | 锁屏、休眠、音量、外观、清空回收站 | 首次使用时询问；重启、关机、注销每次确认 |
| `input.paste` | 向前台应用粘贴文本 | 不询问（与 Raycast 的 `Clipboard.paste` 等同） |
| `ai` | 使用用户配置的模型 | 不询问（与 Raycast 的 `AI.ask` 等同）。改：Raycast 原有的调用方式不需要清单；用到 Heyo 加的选项（`system`、`messages`、`images`、`material`）或 `AI.provider()` 时，清单里必须列出 `ai`，否则以 `denied` 失败 |
| `search.provider` | 向根搜索贡献结果，因而能看到用户输入的每个查询 | 安装时询问 |
| `activity` | 改，新增：在面板的"进行中"区域显示扩展正在做的事，做完时告诉用户（7.9） | 不询问；设置里每个扩展有一个开关，另有总开关，关掉后以 `denied` 失败。只有用户运行的命令能发起新活动 |

### 5.2 询问与存储

- 询问是面板里的一个确认框，写明扩展名和权限的白话说明。改：选项只有"允许""拒绝"，没有"仅这一次"（第 12 节问题 5）。
- 改：只有"允许"写入 `$XDG_CONFIG_HOME/heyo/permissions.json`（按扩展名记录，文件 0600）。"拒绝"不落盘，
  只在内存里记 60 秒：这段时间内同一扩展再问同一权限直接以 `denied` 失败而不弹窗（否则每次渲染都问一次的扩展会把确认框刷屏），
  之后再用再问。设置窗口的扩展页列出已允许的权限并可撤销。
- 同一时间只有一个确认框；两个请求同时要同一权限时，第二个等第一个的答案。五分钟没人回答算作离开，以 `denied` 失败。
- 后台运行（菜单栏、定时命令）时没有人可以回答：未获许可的调用直接以 `denied` 失败，不弹窗。
- 已知偏差（待修）：从 Manage Extensions 卸载扩展时没有清掉它的权限记录；装回同名扩展会沿用。

### 5.3 在哪里检查

- 运行时为每个请求附上发起扩展的名称；主机在执行前查权限表。
- 主机是检查点，因为数据在主机手里。

### 5.4 诚实的边界

今天所有扩展与运行时在同一个 Node.js 进程里，扩展可以读写文件、起子进程。因此：

- 权限系统约束的是 **Heyo 保管的数据和 Heyo 代为执行的动作**，不是扩展在本机能做的一切。
- 一个恶意扩展理论上可以伪造请求来源。v1 的权限是"知情同意和可审计"，不是安全隔离。
- 真正的隔离需要每个扩展一个进程，并由主机按进程识别来源。这列为 v2（见第 11 节），
  规格里的请求格式已为此留出位置：来源由通道决定，而不是由消息里的字段决定。

设置里和安装时的文案必须与此一致，不能把权限描述成沙箱。

---

## 6. 线上协议

### 6.1 请求

扩展请求与内置的 `host.*` 请求分开命名，前缀 `ext.`，这样内部接口可以自由变动。

```jsonc
// 运行时 → 主机
{ "type": "request", "id": 41, "method": "ext.windows.list", "extension": "window-sessions", "params": {} }
// 主机 → 运行时
{ "type": "reply", "id": 41, "result": [ /* … */ ] }
{ "type": "reply", "id": 41, "error": { "code": "denied", "message": "Window access was not allowed." } }
```

- 在 Rust 里是一个新枚举 `ExtRequest`（`#[serde(tag = "method", content = "params")]`），
  与现有 `HostRequest` 并列，同样导出 TypeScript 类型。
- 现有回复里的 `error` 是字符串；`ext.*` 的回复改为 `{ code, message }`。`host.*` 保持不变。

### 6.2 事件

订阅类接口需要主机主动发消息，这是协议里新增的方向：

```jsonc
// 运行时 → 主机：开始订阅
{ "type": "request", "id": 42, "method": "ext.windows.watch", "extension": "…", "params": {} }
// 主机 → 运行时：回复给出订阅号
{ "type": "reply", "id": 42, "result": { "subscription": 7 } }
// 主机 → 运行时：之后的事件
{ "type": "event", "subscription": 7, "data": { "kind": "focused", "window": { /* … */ } } }
// 运行时 → 主机：结束
{ "type": "request", "id": 43, "method": "ext.unsubscribe", "params": { "subscription": 7 } }
```

- 订阅属于发起它的命令会话；会话结束时运行时统一取消，主机在运行时退出时清理。
- 流式结果（听写的中间结果）用同一机制。

### 6.3 错误码

| `code` | 含义 | 扩展该怎么办 |
|---|---|---|
| `unsupported` | 当前会话做不到 | 隐藏功能或说明原因 |
| `denied` | 用户没有许可 | 说明需要什么权限 |
| `cancelled` | 用户中途取消（如退出框选） | 安静结束 |
| `unavailable` | 需要的组件没装（语音引擎、识别引擎） | 引导到设置 |
| `invalid` | 参数不对 | 修正调用 |
| `failed` | 其他失败，`message` 给出原因 | 展示给用户 |

---

## 7. 接口

以下是 `@heyo/api` 在 Raycast 部分之外新增或扩展的内容，签名为 TypeScript。标注"v1"的进入第一版，其余为后续。

### 7.1 环境与能力（v1）

```ts
export const isHeyo: boolean;
export function has(feature: string): boolean;

/** Raycast 的 environment 的全部字段，再加上以下这些。 */
export const environment: RaycastEnvironment & {
  heyoVersion: string;
  apiVersion: number;
  platform: "linux" | "windows";
  session: "x11" | "wayland" | "windows" | "unknown";
  offline: boolean; // 用户开启了"不出本机"
};

export type Support = "full" | "limited" | "none";

export class HeyoError extends Error {
  code: "unsupported" | "denied" | "cancelled" | "unavailable" | "invalid" | "failed";
}
```

`offline` 为真时，扩展应避免联网；Heyo 自己的 `AI` 会只用本地模型。改：Heyo 还没有这个开关，
`offline` 恒为 `false`；要知道模型是否在本机，问 `AI.provider()`（7.8）。

改：第 7 节的每个 Heyo 模块（`Windows`、`Screen`、`Dictation`、`Memory`、`ClipboardHistory`、`Files`、`System`、`LiveActivity`）
都有 `supported(): Promise<Support>`，下文不再逐个写出。`Memory` 和 `ClipboardHistory` 在没有剪贴板监听的会话里答 `"limited"`
（历史仍可读，只是不再增长）；`Files`、`System` 总是 `"full"`；`LiveActivity` 目前总是 `"limited"`（7.9）。
问 `supported()` 不需要权限。

### 7.2 Windows（v1，权限 `windows`）

对应 `platform::windows`。现有内置命令 Switch Windows 和窗口布局改用它。

```ts
export interface Window {
  id: number;
  title: string;
  /** 应用的窗口类，例如 "firefox"。 */
  class: string;
  /** 用户正在用的窗口：面板打开时，是面板出现之前在前台的那个。 */
  active: boolean;
  /** 物理像素。改：已提供；桌面不报告位置时为 undefined。 */
  bounds?: Rect;
  /** 在 monitors() 里的下标。 */
  monitor?: number;
}

export interface Rect { x: number; y: number; width: number; height: number }

export type Layout =
  | "left-half" | "right-half" | "top-half" | "bottom-half"
  | "top-left" | "top-right" | "bottom-left" | "bottom-right"
  | "first-third" | "center-third" | "last-third" | "first-two-thirds" | "last-two-thirds"
  | "almost-maximize" | "maximize" | "center" | "next-display"
  /** 工作区的十二等分：x、y、宽、高。 */
  | { fraction: [number, number, number, number] };

export const Windows: {
  supported(): Promise<Support>;
  /**
   * 改：最近使用的在前，所以第一项就是用户正在用的窗口（`active` 为真），
   * 其余按最近使用排；不含 Heyo 自己的窗口。
   */
  list(): Promise<Window[]>;
  /** Heyo 出现之前在前台的窗口。 */
  active(): Promise<Window | undefined>;
  /** 面板为该窗口让路，与 Heyo 自己的切换一样。 */
  activate(id: number): Promise<void>;
  close(id: number): Promise<void>;
  arrange(id: number, layout: Layout): Promise<void>;
  /** 改：已提供。精确位置，供"保存并恢复布局"使用。 */
  setBounds(id: number, bounds: Rect): Promise<void>;
  monitors(): Promise<Rect[]>;
  /** v1.1（阶段 F）：窗口获得焦点、打开、关闭。返回取消函数。 */
  watch(listener: (event: WindowEvent) => void): () => void;
};
```

说明：`bounds`、`setBounds`、`monitors` 已在三个适配器上实现（X11 实测，GNOME Wayland 和 Windows 只过了编译）。
`watch` 需要 6.2 的事件通道，仍在 v1.1。顺序约定的来由：Switch Windows 要的是"切回上一个"，
它把第一项（当前窗口）挪到列表末尾；接口本身不替命令做这个决定。

### 7.3 Screen（v1，权限 `screen.capture`、`screen.text`）

对应 `platform::capture` 和 `ocr.rs`。

```ts
export type CaptureTarget =
  | "selection"          // 用户框选，复用截图覆盖层
  | "screen"             // 指针所在的显示器
  | { window: number }
  | { rect: Rect };

export interface Capture {
  /** PNG 文件，位于扩展的缓存目录，命令结束后由 Heyo 清理。 */
  path: string;
  width: number;
  height: number;
}

export interface RecognizedText {
  /** 按阅读顺序拼好的全文。 */
  text: string;
  /** v1.1：每一行及其位置。 */
  lines?: { text: string; box: Rect; confidence: number }[];
}

export const Screen: {
  supported(): Promise<Support>;
  capture(target: CaptureTarget): Promise<Capture>;
  /** 识别引擎未安装时以 "unavailable" 失败。 */
  recognizeText(image: string | CaptureTarget): Promise<RecognizedText>;
};
```

- `"selection"` 由用户亲手框选，本身就是同意，所以不另外询问。
- `"screen"`、`{ window }`、`{ rect }` 不经用户操作就能拿到屏幕内容，首次使用时询问。

### 7.4 Dictation（v1，权限 `microphone`）

对应 `dictation.rs`。同一时间只有一段录音；已有录音时调用以 `failed` 失败。

```ts
export interface ListenOptions {
  language?: "auto" | "zh" | "en" | "ja" | "ko" | "yue";
  /** 是否显示 Heyo 的听写浮窗。默认显示。 */
  indicator?: boolean;
  /** 每识别完一句调用一次。 */
  onSegment?: (text: string) => void;
  signal?: AbortSignal;
}

export const Dictation: {
  supported(): Promise<Support>;
  installed(): Promise<boolean>;
  /** 开始录音，直到调用返回对象的 stop()、用户按热键结束，或 signal 取消。 */
  listen(options?: ListenOptions): { stop(): void; result: Promise<string> };
  /** v1.1：转写一个音频文件（16 kHz 单声道 WAV）。 */
  transcribe(file: string, options?: { language?: string }): Promise<string>;
};
```

与现有听写的区别：`listen()` 把文字**交给扩展**，不粘贴到前台应用。

### 7.5 Memory 与 ClipboardHistory（v1，权限 `memory.read`、`memory.write`、`clipboard.history`）

`Memory` 是跨来源的记忆搜索，对应今天的 `clipboard/store.rs` 和 `clipboard/vectors.rs`，随记忆搜索后续阶段扩展来源；
v1 里唯一的来源就是剪贴板。改：`ClipboardHistory` 不再是另一个读取入口——读历史就是 `Memory.search`
（来源只有剪贴板时无需筛 `sources`）；它剩下的是只有剪贴板才有的动作：把一条放回剪贴板。
Heyo 自己的 Clipboard History 命令就是这样写的（固定、删除、清空、暂停记录仍是它的私有请求，因为那是"保管历史"而不是"读历史"）。

```ts
export type MemorySource = "clipboard" | "screenshot" | "dictation" | "file" | "web" | `ext:${string}`;

export interface MemoryItem {
  /** 来源加来源里的编号，如 "clipboard:412"。 */
  id: string;
  source: MemorySource;
  kind: "text" | "link" | "color" | "file" | "image";
  text?: string;
  /** 图像条目的 PNG 文件。 */
  image?: string;
  /** 改：图像的像素尺寸。 */
  width?: number;
  height?: number;
  /** 来自哪个应用（窗口类）。 */
  app?: string;
  /** 改：该应用的名称（没有对应已安装应用时就是窗口类）和图标文件。 */
  appName?: string;
  appIcon?: string;
  /** 最后一次复制的毫秒时间戳。 */
  at: number;
  /** 改：首次复制的时间和复制次数。剪贴板来源会计数；别的来源不一定有，所以可选。 */
  firstAt?: number;
  timesCopied?: number;
  pinned: boolean;
  /** 改：按语义找到的——查询的词并不在条目里。 */
  byMeaning?: boolean;
}

export interface MemoryQuery {
  /** 自然语言查询，可含时间、来源、类型（"上周 从 chrome 复制的 链接"）。 */
  text?: string;
  sources?: MemorySource[];
  kinds?: MemoryItem["kind"][];
  since?: Date;
  until?: Date;
  limit?: number; // 默认 50，改：上限 300（Heyo 自己的历史视图显示的条数）
  /**
   * 改：也找与 `text` 意思相同但不含其词的条目，前提是用户在设置里开了"按语义查找"
   * （没开时此项无效）。结果等它：首次一两秒（加载模型），之后几毫秒。找到的标 `byMeaning`。
   */
  byMeaning?: boolean;
}

export interface MemoryResult {
  items: MemoryItem[];
  /** 查询被读成了什么，供界面显示"正在搜索：上周 · 链接"。 */
  understood: { terms: string[]; app?: string; kind?: MemoryItem["kind"]; from?: number; to?: number };
  /** 读出的条件没有匹配，已按纯文字重新搜索。 */
  relaxed: boolean;
}

export const Memory: {
  supported(): Promise<Support>;
  /** 改：query 可省略：没有 `text` 就是最近的。 */
  search(query?: MemoryQuery): Promise<MemoryResult>;
  get(id: string): Promise<MemoryItem | undefined>;
  /** 待定（第 12 节问题 4），v1 未实现：写入扩展自己的条目，来源记为 `ext:<扩展名>`；只能删除自己写入的。 */
  add(item: { text: string; kind?: MemoryItem["kind"]; at?: Date; meta?: Record<string, string> }): Promise<string>;
  remove(id: string): Promise<void>;
};

export const ClipboardHistory: {
  supported(): Promise<Support>;
  /** 把一条历史放回剪贴板。 */
  copy(id: string): Promise<void>;
};
```

- 被标记为机密的复制内容（`concealed`）从不进入历史，因此也不会被返回。
- `Memory.search` 不提供"导出全部"：没有 `text` 且没有时间范围的查询最多返回 `limit` 条最近的。
- 改：`ClipboardHistory.search(text, { kind, limit })` 去掉了。它只是 `Memory.search({ text, kinds: [kind], limit })` 的子集，
  内置命令没有用到它，两个读取入口只会让权限表多一行要解释的东西。`has("ClipboardHistory.search")` 为假。

### 7.6 Files（v1，权限 `files.search`）

对应 `files.rs`，只返回路径，不读内容。

```ts
export const Files: {
  supported(): Promise<Support>;
  search(query: string, options?: { limit?: number }): Promise<{
    files: { path: string; isDirectory: boolean }[];
    /** 索引仍在扫描，结果可能不全。 */
    scanning: boolean;
    /** 改：索引里一共有多少个文件和目录，供界面显示"在 183,204 项中搜索"。 */
    total: number;
  }>;
};
```

### 7.7 System 与 Clipboard（v1，权限 `system`、`input.paste`）

对应 `platform/system.rs` 和 `platform/input.rs`。

```ts
export type SystemAction =
  | "lock-screen" | "sleep" | "restart" | "shut-down" | "log-out"
  | "empty-trash" | "toggle-dark-mode" | "volume-up" | "volume-down" | "toggle-mute";

/** 改：扬声器或麦克风。 */
export interface AudioDevice {
  /** 声音服务器里的名字；useAudioDevice 用它。 */
  name: string;
  /** 给用户看的描述。 */
  description: string;
  kind: "output" | "input";
  /** 声音现在是否走它。 */
  current: boolean;
}

export const System: {
  supported(): Promise<Support>;
  /** 返回值是可以展示给用户的一句话（"Volume 40%"），没有则为 undefined。 */
  run(action: SystemAction): Promise<string | undefined>;
  appearance(): Promise<"light" | "dark">;
  /** 短暂的桌面通知。 */
  notify(title: string, body?: string): Promise<void>;
  /** 改：扬声器和麦克风，各标出当前在用的。 */
  audioDevices(): Promise<AudioDevice[]>;
  /** 改：按 `name` 切换声音走哪个设备。 */
  useAudioDevice(kind: AudioDevice["kind"], name: string): Promise<void>;
};
```

`restart`、`shut-down`、`log-out` 会结束会话，由主机弹确认框，扩展不能绕过：每次都问，不记答案；
用户选"否"时以 `cancelled` 失败。改：Heyo 自己的系统命令已经用自己的措辞问过一次，主机不再问第二次。
`appearance` 和 `notify` 需要 `system` 权限但不询问（只读、无害）。
改：音频设备原本是 Heyo 自己的命令直接调 `pactl` 做的，阶段 E 把它收进主机并开放给扩展；结束进程没有收进来（它不在权限表里，仍是私有请求）。

粘贴和读取选中文字 Raycast 已有接口（`Clipboard.paste`、`getSelectedText`），不另起名字，
只在原接口上加可选参数：

```ts
export const Clipboard: RaycastClipboard & {
  /** Raycast 的签名不变；多一个可选项：粘贴后把剪贴板恢复成原来的内容。 */
  paste(content: string | Clipboard.Content, options?: { restoreClipboard?: boolean }): Promise<void>;
};
```

终端里粘贴用 Ctrl+Shift+V，这由主机判断，扩展不需要知道。

### 7.8 AI（v1，权限 `ai`）

在 Raycast 的 `AI` 上扩展。`AI.ask(prompt, { model, creativity, signal })` 的签名和返回值
（一个同时能 `.on("data", …)` 的 Promise）保持不变。改：Heyo 自己的 Ask AI、AI Chat 和选中文字操作
就是用这个 `ask` 写的（以 `@heyo` 的身份，见 5.4 和 `permissions::identity`），运行时里不再有另一条到模型的路。

```ts
export const AI: RaycastAI & {
  /** 当前配置：用的是哪家、是否本地。供扩展决定要不要发送敏感内容。 */
  provider(): Promise<{ kind: "claude" | "local"; model: string; local: boolean }>;
  ask(prompt: string, options?: RaycastAI.AskOptions & {
    /** 代替 Heyo 给扩展的默认指令（"你是桌面启动器扩展里的助手……"）。 */
    system?: string;
    /** 图像文件路径（PNG、JPEG、GIF、WebP）；模型不支持图像时以 "unsupported" 失败。 */
    images?: string[];
    /**
     * 改：到目前为止的对话，最早的在前，一问一答交替；prompt 是用户的下一句。
     * 不交替、或者不以回答结尾，以 "invalid" 失败。
     */
    messages?: AI.Message[];
    /**
     * 改：prompt 是要处理的文字（翻译、改写……）而不是要回答的话：它会被围起来当作材料，
     * 模型按 `system` 做事而不听材料里的话（本地小模型尤其需要这一层）。不能与 `messages` 同用。
     */
    material?: boolean;
    /** 改：每次有新内容时拿到目前为止的整段回答（`on("data")` 给的是增量）。 */
    onText?: (text: string) => void;
  }): Promise<string> & { on(event: "data", listener: (chunk: string) => void): void };
};

export namespace AI {
  /** 改：对话里先前的一问或一答。 */
  interface Message { role: "user" | "assistant"; content: string }
}
```

- 改（已实现）：`creativity` 映射到本地模型的温度（`none` 0、`low` 0.5、`medium` 1、`high` 1.5、`maximum` 2，或 0–2 的数字）；
  Claude 不带温度。`model` 被接受但仍按用户在设置里选的模型执行，因为 Raycast 的模型名在这里没有对应物。这是一处有文档的降级。
- `images` 让"截屏后提问"成为两行：`Screen.capture("selection")`，然后
  `AI.ask(question, { images: [capture.path] })`。
- 改：三种用法对应三种发给模型的形状。只有 `prompt`（或加 `system`）：一条用户消息；`messages`：
  先前的轮次原样在前，`prompt` 是新的一条——AI Chat 就是这样把整段对话交给模型的；`material`：
  `prompt` 不是消息，而是围在 `<text>` 里的材料，`system` 说明对它做什么——选中文字操作就是这样做的。
- 改：失败一律是 `HeyoError`：`signal` 取消是 `cancelled`；参数不对是 `invalid`；本地模型不收图像是 `unsupported`；
  其余是 `failed`，`message` 是给用户看的一句话（"No Claude API key is set. Add one in Heyo Settings → Advanced."）。
  Raycast 的 `ask` 不规定失败的形状，所以这不破坏兼容。
- 改：流式的节奏与 Heyo 自己的命令一样：模型每吐一段，`onText` 和 `on("data")` 就各被叫一次；界面何时重绘由调用方决定
  （AI Chat 每 80 毫秒取一次，长对话里只有正在写的那条回答会被重发）。
- 面板关闭后回答继续生成并写进对话（这是 Ask AI 和 AI Chat 的行为，不是接口规定的；第 12 节之外的待决项，见 `TODO.md`）。
- Heyo 之外（`packages/api/index.js`）：`system`、`messages` 的转写和 `material` 的围栏都拼在 prompt 前面交给 Raycast 的 `ask`；
  `onText` 由 `on("data")` 累加；`images` 以 `unsupported` 拒绝。

### 7.9 LiveActivity（v1，权限 `activity`）

改，新增。对应 `activities.rs`。命令不在面板里的时候，由 Heyo 替它显示"正在进行的事"：面板的"进行中"区域，
以及（桌面支持时）状态栏。计时和按时间推进的进度由 Heyo 自己走，所以命令不必一直运行。
设计的来由、各桌面的呈现和上限的依据在 `docs/live-activities.md`（接口是它的第 6 节）；这里只写接口。

```ts
export namespace LiveActivity {
  /** 不变的部分。 */
  interface Attributes {
    /** 同一扩展内唯一。再次 start 同一个 id 等于更新。 */
    id: string;
    /** 这是什么（"Build"、"Timer"）。 */
    name: string;
    icon: Image.ImageLike;
    tint?: Color.ColorLike;
    /** 选中活动本身时做什么。 */
    action?: Action;
    /** 用户能否结束它。默认 true。 */
    dismissible?: boolean;
    /** 这个桌面上显示不了时怎么办。默认 "notify"。 */
    fallback?: "notify" | "drop";
  }

  /** 会变的部分。 */
  interface Content {
    /** 折叠态里的内容。缺省时由 title 截断得到。 */
    compact?: Value;
    title: string;
    subtitle?: string;
    /** 展开态的主体，三选一。 */
    body?: { progress: Progress } | { timer: Timer } | { text: string };
    /** 最多三个；出现在展开态和面板的动作面板里。 */
    actions?: Button[];
    /** 需要用户处理（确认、回答）：排到前面并标出来。 */
    needsAttention?: boolean;
    /** 这之后内容可能不准，显示时变淡。不会结束活动。 */
    staleAt?: Date;
    /** 同一个扩展的多个活动之间的次序，0–100。 */
    relevance?: number;
  }

  /** 一小段会自己变的内容。 */
  type Value = string | { timer: Timer } | { progress: Progress } | { icon: Image.ImageLike };

  /** 一个时间区间加一个暂停时刻，由 Heyo 走。 */
  interface Timer {
    start: Date;
    /** 省略表示正计时，没有终点。 */
    end?: Date;
    /** 暂停在哪一刻；省略表示在走。 */
    pausedAt?: Date;
    /** 有终点时默认倒数。 */
    countsDown?: boolean;
    /** "12:05" 或 "12 min"。 */
    style?: "clock" | "relative";
  }

  interface Progress {
    /** 0–1。省略且没有 interval 时为不确定进度。 */
    value?: number;
    /** 按时间自己推进。 */
    interval?: Pick<Timer, "start" | "end" | "pausedAt">;
  }

  interface Button {
    title: string;
    icon?: Image.ImageLike;
    style?: "regular" | "destructive";
    action: Action;
  }

  /** 前三种由 Heyo 直接执行；`event` 送回扩展。 */
  type Action =
    | { open: string }
    | { copy: string }
    | { launch: { command: string; arguments?: Record<string, string>; context?: unknown } }
    | { event: string; data?: unknown }
    | { end: true };

  interface StartOptions {
    /** 出现时短暂展示一次。默认 true。 */
    announce?: boolean;
  }

  interface Alert {
    /** 短暂展示这次更新。 */
    expand?: boolean;
  }

  interface Handle {
    readonly id: string;
    /** 每个活动每秒一次，超出的合并。 */
    update(content: Partial<Content>, options?: { alert?: Alert }): Promise<void>;
    /** 最终内容显示片刻（"brief"，默认）或立即消失（"none"）。 */
    end(final?: Partial<Content>, options?: { keep?: "none" | "brief" }): Promise<void>;
  }

  type Event =
    | { kind: "action"; id: string; event: string; data?: unknown }
    | { kind: "dismissed"; id: string }
    | { kind: "expired"; id: string }
    | { kind: "elapsed"; id: string; at: number };

  interface Presentation {
    /** 指状态栏；面板里的区域总在。 */
    support: Support;
    /** 折叠态能放多少字。 */
    compactCharacters: number;
    /** 展开态是否显示，它的按钮是否显示。 */
    expanded: boolean;
    buttons: boolean;
    /** 版式的版本，以后加版式时递增。 */
    version: number;
  }
}

export const LiveActivity: {
  supported(): Promise<Support>;
  /** 发起一个活动，或更新同 id 的那个。 */
  start(attributes: LiveActivity.Attributes, content: LiveActivity.Content, options?: LiveActivity.StartOptions): Promise<LiveActivity.Handle>;
  /** 本扩展正在进行的活动。 */
  list(): Promise<LiveActivity.Handle[]>;
  get(id: string): Promise<LiveActivity.Handle | undefined>;
  /** 这个桌面上活动显示成什么样，扩展可据此调整内容。 */
  presentation(): Promise<LiveActivity.Presentation>;
  /** 命令运行期间收本扩展活动的事件。返回取消函数。 */
  onEvent(listener: (event: LiveActivity.Event) => void): Promise<() => void>;
};
```

每个函数发的请求和要的权限（取自 `runtime/src/heyo/members.ts` 的声明和主机的 `needed`）：

| 函数 | 请求 | 权限 |
|---|---|---|
| `supported()` | `ext.supported`（`module: "activity"`） | 无 |
| `start` | `ext.activity.start` | `activity` |
| `list` | `ext.activity.list` | `activity` |
| `get` | `ext.activity.get` | `activity` |
| `presentation` | `ext.activity.presentation` | 无 |
| `onEvent` | `ext.activity.onEvent`（订阅，见 6.2） | `activity` |
| `Handle.update`、`Handle.end` | `ext.activity.update`、`ext.activity.end` | `activity` |

- **权限**：清单里列出 `activity` 即可，首次使用不询问。用户在设置里可以按扩展关掉，也可以整个关掉；关掉后以 `denied` 失败，
  Heyo 自己的活动也一样。没列权限时主机的拒绝写的是作者调用的那个名字：
  `LiveActivity.start needs the "activity" permission; add it to "heyo.permissions" in package.json.`
- **谁能发起**：只有用户运行的命令，或用户点了活动上的按钮之后被运行的处理命令。定时在后台运行的命令 `start` 一个新活动
  以 `denied` 失败，更新自己已有的活动不受限。运行时把命令是怎么启动的（`launchType`）写进请求，扩展不传。
- **上限**：每个扩展同时 3 个活动，属性加内容共 4 KB，按钮 3 个，超出以 `invalid` 失败；折叠态约 10 个字，超出截断
  （确切值问 `presentation().compactCharacters`）。
- **先后**：线上的日期是毫秒。每次 `start` 和 `update` 由运行时带一个只增的序号，主机丢掉序号不比已应用的大的更新，
  所以发起活动的命令和处理按钮的命令同时更新时，旧的不会盖掉新的。句柄不论在哪里被调用，都以创建它的那个命令的名义发请求。
- **事件**：正在运行的命令用 `onEvent` 订阅；没有命令在听时，主机在后台运行清单里标了 `"heyo": { "handles": "activity" }` 的命令，
  事件在它的 `launchContext.activity` 里。`elapsed` 是倒计时走到头。
- **现状**：`supported()` 答 `"limited"`，`presentation()` 答 `support: "none"`、`expanded` 和 `buttons` 为假——活动显示在面板的
  "进行中"区域，需要用户知道的事另发通知；状态栏还没有画。
- Heyo 之外（`packages/api/index.js`）：`supported()` 答 `"none"`，其余以 `unsupported` 拒绝。

### 7.10 根搜索结果提供者（v1.1，权限 `search.provider`）

Raycast 没有的能力：扩展不必被打开，就能让自己的条目出现在根搜索里。

清单里声明一个 `provider` 模式的命令：

```json
{ "name": "issues", "title": "Issues", "mode": "provider", "heyo": { "debounce": 120, "minLength": 2 } }
```

命令文件默认导出一个函数：

```ts
import type { SearchProvider } from "@heyo/api";

const provider: SearchProvider = async (query, { signal }) => [
  {
    id: "HEYO-12",
    title: "Crash when the palette is toggled twice",
    subtitle: "HEYO-12 · open",
    icon: "bug-16",
    /** 0–100；与内置条目的模糊匹配分数同一量纲。 */
    score: 70,
    action: { open: "https://example.com/HEYO-12" },
  },
];
export default provider;
```

约束，都是为了不拖慢根搜索：

- 提供者在后台运行时进程里执行，与面板里的命令互不影响。
- 每次按键不等待提供者：先显示内置结果，提供者的结果在 250 ms 内到达则并入，超时丢弃。
- 新的查询到来时，上一次的 `signal` 被取消。
- 每个提供者每次最多 5 条；条目归在以扩展标题命名的小节下，位于内置结果之后，除非分数更高。
- 连续 5 次超时或抛错的提供者在本次会话里停用，并在设置里标出。
- `action` 只能是：打开链接或文件、复制文本、粘贴文本、启动本扩展的另一个命令（可带参数）。不能是任意函数，因为结果要跨进程传给界面。

### 7.11 后续（不在 v1）

| 接口 | 用途 | 依赖 |
|---|---|---|
| `Clipboard.watch` | 复制时触发扩展 | 6.2 的事件通道、后台进程 |
| `Hotkeys.onHold` | 扩展自己的"按住说话"式交互 | 权限与冲突处理的设计 |
| `Overlay` | 扩展在屏幕上画东西（标尺、取色） | 新的窗口类型 |
| `Browser.tabs` | 标签页搜索 | 配套的浏览器扩展 |

---

## 8. Raycast 接口的覆盖

"换一个 import 就能迁移"要求 Raycast 部分没有缺口。下表是按 `runtime/src/api/` 现在的
实现整理的现状。它是导出级别的；每个组件的每个属性是否都实现，我没有逐一核对，
那正是 8.3 的一致性测试要自动回答的。

### 8.1 现状

| 导出 | 现状 | 要补什么 |
|---|---|---|
| `List`、`Grid`、`Detail`、`Form`、`ActionPanel`、`MenuBarExtra`、`useNavigation` | 已实现 | 属性级核对（8.3） |
| `Action` 及 `CopyToClipboard`、`Paste`、`Open`、`OpenInBrowser`、`OpenWith`、`ShowInFinder`、`Trash`、`Push`、`SubmitForm` | 已实现 | `OpenWith` 目前不让用户选应用 |
| `Action.CreateSnippet`、`Action.CreateQuicklink` | 已实现。改：打开 Heyo 自己的创建片段、创建快捷链接表单，按传入的内容填好 | 快捷链接的 `application` 和 `icon` 没有去处（一律用默认应用打开） |
| `Action.PickDate` | 已实现。改：在动作面板里选日期 | — |
| `Action.ToggleQuickLook` | 已实现。改：图像和文本在压到列表上的一个视图里显示 | 其他类型的文件交给桌面的默认应用打开 |
| `showToast`、`Toast`、`showHUD`、`confirmAlert`、`Alert` | 已实现 | `showHUD` 的选项被忽略 |
| `Clipboard`（`copy`、`paste`、`clear`、`read`、`readText`） | 已实现 | — |
| `LocalStorage`、`Cache` | 已实现 | — |
| `getPreferenceValues`、`openExtensionPreferences`、`openCommandPreferences` | 已实现 | — |
| `OAuth.PKCEClient` | 已实现 | — |
| `open`、`trash`、`showInFinder`、`getApplications`、`getDefaultApplication` | 已实现 | `getApplications(path)` 的参数被忽略 |
| `getSelectedText` | 已实现 | — |
| `closeMainWindow`、`popToRoot`、`clearSearchBar` | 已实现 | 选项（`clearRootSearch`、`popToRootType`）被忽略 |
| `launchCommand` | 已实现。改：也能启动其他已安装扩展的命令，首次由用户同意（面板里问一次，按发起方记住；菜单栏和后台里没人可问，未获同意时失败） | 带视图的命令不能在后台启动；只有面板里的那个命令，或用户点了菜单项的菜单栏命令，能把别的命令开进面板 |
| `environment` | 已实现。改：`appearance` 跟着 Heyo 界面的明暗走（运行时启动时由主机告知，之后随设置变化更新） | `raycastVersion` 是固定字符串；`textSize` 恒为 `"medium"`；`launchContext` 和三个 `entryPoint*` 字段还没有（8.3） |
| `AI.ask` | 已实现（改：含 `creativity` → 温度，以及 7.8 的 `system`、`messages`、`images`、`material`、`onText`） | `model` 按设置里的模型执行；见 7.8 |
| `Icon`、`Color`、`Image`、`Keyboard` | 已实现 | 图标表随 Raycast 版本更新 |
| `getFrontmostApplication` | 已实现（2026-10-10） | 找不到对应的已安装应用时，只返回窗口类名 |
| `WindowManagement`（4 个函数） | 已实现（2026-10-10），X11 上实测 | "桌面"按显示器算；`"fullscreen"` 是最大化；单位是物理像素；窗口的 `application` 字段未填 |
| `updateCommandMetadata` | 空操作 | 让根搜索显示命令的动态副标题 |
| `getSelectedFinderItems` | 返回空数组 | 文件管理器里选中的文件；GNOME 上需经 Shell 扩展，其余会话返回空 |
| `BrowserExtension` | 抛错 | 需要配套的浏览器扩展（7.11） |
| `Tool`（AI 扩展的工具确认） | 没有 | 取决于是否支持 Raycast 的 AI 扩展 |
| 已弃用的导出（`copyTextToClipboard`、旧的 Action 组件、`getLocalStorageItem` 等） | 已实现。改：作为现有实现的别名补齐（8.3） | — |

### 8.2 做不到同行为的地方

以下是 macOS 的概念，在 Heyo 上给出确定的降级，写进文档：

| 接口 | Heyo 上的行为 |
|---|---|
| `Application.bundleId` | 桌面文件名（Linux）或程序路径（Windows） |
| `showInFinder` | 用文件管理器打开所在目录 |
| `Action.ShowInFinder` 的标题 | "Show in Files" |
| `Keyboard.Shortcut` 里的 `cmd` | 映射为 Ctrl（现已如此） |
| `environment.raycastVersion` | 一个固定的、高于扩展常见最低要求的版本号 |
| `AI.ask` 的 `model` | 忽略，用设置里的模型 |
| `getDesktops` | 每个显示器算一个桌面；没有全屏桌面 |
| `setWindowBounds` 的 `"fullscreen"` | 最大化 |
| 窗口的位置和尺寸 | 物理像素（macOS 上是点） |

### 8.3 一致性测试

**现状（2026-10-10，对照 `@raycast/api` 2.7.2）**：名字级别的测试已经写好并通过，位于
`runtime/src/api/conformance.test.ts`。它比较每个导出及其成员的名字，深入三层，
不比较函数签名和组件属性。首次运行找出的缺口里，旧版别名（`CopyToClipboardAction`、
`FormTextField`、`getLocalStorageItem` 等 37 个）、`AI.Model` 的取值和 `Color.Brown` 已经补上。
仍然缺的 16 个名字记在 `conformance.gaps.json` 里，分三类：

- 旧式入口 `render()`（1 个）。
- Raycast 标为不稳定或内部使用的（7 个）：`unstable_AI`、`useUnstableAI`、`AI.experimental_decide`、
  `AI.refreshModels`、`captureMemorySnapshot`、`specialKeys`、`useActionPanel`。
- 较新的、还没有研究的（8 个）：`environment.launchContext` 和三个 `entryPoint*` 字段、
  OAuth 的客户端元数据文档方式、`Cache` 的两个静态常量。

注意名字存在不等于行为完整：8.1 里"抛错"和"提示不支持"的那些名字都在，所以不在这 16 个里。

**组件属性**：第二个测试 `runtime/src/api/props.test.ts` 从类型里取出每个 Raycast 组件的属性名，
检查渲染组件的代码里有没有出现过。从未出现的属性一定是被忽略的。首次运行后修掉了三处
（旧属性名 `enableFiltering`、`accessoryTitle`、`accessoryIcon`，以及 `Action.CopyToClipboard` 的 `transient`），
剩下的记在 `props.gaps.json`。改：现在只剩三个——`Form.enableDrafts`、`Form.TextArea.enableMarkdown`、
`MenuBarExtra.Item.alternate`；当初记下的 `storeValue`、`Form.FilePicker` 的选项、`quickLook` 和 `Action.CreateSnippet.snippet` 后来都接上了，已不在清单里。

这个测试只能找出"肯定没处理"的属性。名字出现过的属性仍可能只处理了一部分，那要靠逐个组件对照，
数不出来。

函数签名的比对还没有做：

- 真正的 `@raycast/api` 已作为开发依赖装入，只用它的类型声明。
- 一个类型测试断言我们的实现可以赋值给它：
  `const check: typeof import("@raycast/api") = heyoApi;`。少一个导出、少一个函数、参数不对，
  `tsc` 就失败。组件的属性用同样的办法逐个组件比对。
- 降级和暂缺的项列在一个允许清单里，清单里的每一项必须对应 8.1 或 8.2 的一行。
- Raycast 发新版本时升级这个开发依赖，测试失败的地方就是新增的接口。

### 8.4 权限

Raycast 部分的接口不要求 `heyo` 清单字段，否则商店里的扩展都跑不了。它们映射到的权限
（`WindowManagement` → `windows`）同样在首次使用时询问，这样没有清单的扩展不能绕过用户。

---

## 9. 开发者体验

- `heyo migrate <目录>`：把 `@raycast/api` 的 import 换成 `@heyo/api`，加上 `heyo` 清单字段。
- `heyo build <目录>`：构建扩展，产物与 `ray build` 相同（每个命令一个 `.js`，接口和 React 留给运行时提供）。
  Raycast 自己的构建工具 `ray` 能否在 Linux 上运行我没有核实；有 `heyo build` 就不依赖它。
- `heyo dev <目录>`：监听并构建扩展，装入正在运行的 Heyo，运行时错误显示在终端。
- `heyo ext check <目录>`：检查清单、`apiVersion`、声明的权限是否覆盖代码里用到的接口；
  并找出接口之外的移植问题（调用 macOS 程序、AppleScript、写死的 macOS 路径）。
- 错误信息面向扩展作者时包含接口名和所需权限，例如
  `Windows.list needs the "windows" permission; add it to "heyo.permissions" in package.json.`
- 示例扩展放在 `examples/`，每个模块一个，同时作为端到端测试的对象。

---

## 10. 测试

| 层 | 怎么测 |
|---|---|
| 请求格式 | `wire.rs` 的单元测试：每个 `ext.*` 的读入、未知方法被拒绝 |
| 权限判定 | 纯函数 `permitted(extension, permission, grants) -> Decision`，表驱动测试 |
| 平台行为 | 用现有的 `desktop::fake::Fake`，断言主机向桌面问了什么 |
| Raycast 一致性 | 8.3 的类型测试，在 `tsc` 里运行 |
| 运行时垫片 | vitest：`@heyo/api` 的每个函数发出的请求与类型一致；Heyo 之外的桩行为 |
| 迁移 | 取几个商店扩展，跑 `heyo migrate` 和 `heyo build`，断言改 import 前后行为相同 |
| 端到端 | `scripts/gui-test/` 里装入 `examples/` 的扩展，在沙箱里跑 |
| 提供者时限 | 一个故意慢的提供者，断言根搜索不被拖慢、并在多次超时后停用 |

---

## 11. 实现计划

改：A–E 已完成（2026-10-10 至 10-11），表里加了"状态"一列。

| 阶段 | 状态 | 内容 | 涉及的文件 |
|---|---|---|---|
| A | 完成 | `ExtRequest` 枚举、`{ code, message }` 错误、请求带扩展名和目录 | `wire.rs`、`ext.rs`、`runtime.rs`、`runtime/src/protocol.ts` |
| B | 完成 | 权限：清单读取、判定函数 `decide`、询问框、存储、设置页；拒绝记 60 秒（5.2） | `permissions.rs`、`extensions.rs`、`src/Settings.tsx`、`src/App.tsx` |
| C | 完成 | `@heyo/api` 垫片与类型包；`provided` 表里两个包名指向同一对象；类型测试 `types.check.ts` 把 `index.d.ts` 和运行时扣在一起 | `runtime/src/heyo/`、`runtime/src/runtime.ts`、`packages/api/` |
| C2 | 完成 | Raycast 一致性测试（8.3），并按它补齐 8.1 的缺口 | `runtime/src/api/conformance.test.ts`、`props.test.ts` |
| C3 | 完成 | `heyo build`、`heyo migrate`、`heyo ext check` | `packages/cli/` |
| D | 完成 | v1 模块：Windows、Screen、Dictation、Memory、Files、System、Input、AI | `ext.rs` 调用现有 `platform::*`、`clipboard`、`files`、`ocr`、`dictation` |
| E | 完成 | 内置命令改用 `@heyo/api`：切换窗口、系统命令、音频设备、文件搜索、剪贴板历史的读取与复制，删掉 15 个 `host.*`；v0.3 又把 Ask AI、AI Chat、选中文字操作改走 `AI.ask` | `runtime/src/builtins/*`、`host.rs` |
| F | 未做 | 事件通道（6.2）：`Windows.watch`，以及 7.11 里凡是"主机主动发消息"的接口。`bounds`、`setBounds` 已随 D 做完，不再属于 F | `wire.rs`、`platform/desktop.rs` 及三个适配器、GNOME 扩展、`runtime/src/heyo/events.ts`（订阅的运行时一侧已有） |
| G | 未做 | 根搜索提供者（7.10）：`provider` 命令模式、`search.provider` 权限、后台进程里的时限与停用 | `root.rs`、`runtime/src/background.ts`、`src/RootSearch.tsx` |
| H（v2） | 未做 | 每个扩展一个进程，主机按进程识别来源；权限才成为边界（5.4）。`ExtCaller` 的"来源由通道决定"已为此留位 | `runtime.rs`、`helper.rs` |

A–E（含 C2、C3）是 v1，F–G 是 v1.1。阶段 E 验证了设计：内置命令用这套接口写得顺的地方照旧，写不顺的地方都改成了接口
（`MemoryItem` 的字段、`byMeaning`、`total`、音频设备、`AI.ask` 的 `messages` 和 `material`），本版规格就是那份清单的结果。
没有照搬进接口的：固定/删除/清空历史、结束进程、对话的保存——它们是"保管"而不是"读取"，留作 Heyo 的私有请求。

---

## 12. 待决问题

1. **包名**：`@heyo/api` 需要在 npm 上注册 `@heyo` 组织，是否可用我没有核实。备选 `heyo-api`。改：包还没发布（`"private": true`），所以还不急。
2. **对没有 `heyo` 清单的商店扩展**：它们通过 Raycast 接口用到窗口时，是首次询问（本文的选择），还是默认允许以减少打扰？改：现在的实现是首次询问（`Route::Raycast`，8.4）。
3. **`search.provider` 能看到每次输入**：是否接受安装时一次性同意，还是要求用户在设置里逐个开启？（阶段 G 之前要定。）
4. **`Memory.add`**：是否在 v1 就开放写入？它让扩展把自己的数据放进统一搜索，但也让索引的内容不再全由 Heyo 决定。改：v1 没有实现，`has("Memory.add")` 为假。
5. ~~**权限的"仅这一次"**~~ 改：已由实现回答——只有"允许/拒绝"，拒绝记 60 秒（5.2）。要改回三个选项，`permissions.rs` 的 `Decision` 要多一种。
6. **进程隔离（阶段 H）提前到 v1**：代价是每个扩展多一个 Node.js 进程的内存（实测前我估不出具体数字），好处是权限成为真正的边界。
7. ~~**Raycast 部分的类型**~~ 改：已由实现回答——转导出（`packages/api/index.d.ts` 里 `export * from "@raycast/api"`，`@raycast/api` 是 peerDependency）。
8. **Raycast 的 AI 扩展（`Tool`）**：是否在 v1 支持。它是 Raycast 较新的一类扩展，不支持的话"全部能力"要打一个折扣。
9. **`BrowserExtension`**：需要我们自己发布浏览器扩展才能实现。是否纳入 v1，还是列为已知缺口。改：现在是已知缺口，按名字存在、调用即抛错。
10. 改，新增：**`AI.ask` 的 `material`**。它是为选中文字操作加的（本地小模型会听材料里的指令，需要围栏和复述任务），
    扩展同样会遇到这个问题。但 Raycast 没有对应物，在 Raycast 里只能退化成在 prompt 前后加围栏。是保留，还是让扩展自己拼围栏？

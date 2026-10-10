# 待办：对标 Raycast 常用功能

按 Raycast 的功能分区整理。`[x]` 已完成，`[ ]` 待做。
优先级：**P0** 高频且 Linux 上可行，**P1** 常用，**P2** 锦上添花或受平台限制。

## 根搜索与启动

- [x] 模糊搜索应用（`.desktop`）和命令
- [x] 单实例 + 全局热键（GNOME 自定义快捷键，设置里可录制）
- [x] 失焦隐藏、重新打开时保留当前命令
- [x] 按使用频率排序（frecency）：常用的应用和命令排在前面，空查询时显示「Suggestions」
- [x] 收藏（Favorites）：`Ctrl+Shift+F` 固定到根搜索顶部，可调整顺序
- [x] 别名（Alias）：`Ctrl+K → Set Alias`，如 `cb` → Clipboard History
- [x] 每个应用 / 命令 / 快捷链接 / 片段单独的全局热键（根搜索 `Ctrl+Shift+H`，或设置 → Extensions 的 Hotkey 列）
- [x] 打开已在运行的应用时切到它的窗口而不是再开一个；再按一次在该应用的多个窗口间轮换
- [ ] **P2** 根搜索里给应用加「新建窗口」操作
- [x] 回退命令：结果下方提供「用 Google 搜索」「搜索文件」和带 `{query}` 的快捷链接
- [ ] **P2** 回退命令可在设置里排序和开关
- [x] **P1** 命令参数直接在根搜索栏输入（Raycast 的内联参数），而不是弹出表单
- [ ] **P2** 搜索文件夹/最近文档

## 内置工具

- [x] 剪贴板历史（文本/链接/颜色/文件/图片、固定、搜索、按类型筛选、跳过密码）
- [x] 计算器：`2^10`、`15% of 80`、单位换算 `5 km in mi`、货币 `100 usd in cny`、时区 `3pm pst in beijing`
- [x] 快捷链接（Quicklinks）：带 `{query}` 的模板；设别名后输入 `gh heyo` 直接打开
- [x] 文本片段（Snippets）：`{clipboard}`、`{date}`、`{argument}` 等占位符，关键词在根搜索直接粘贴
- [x] 系统命令：锁屏、睡眠、重启、关机、注销、清空回收站、切换深色模式、调节音量、防休眠
- [x] 窗口切换（Switch Windows）：搜索并切换到已打开的窗口，可关闭窗口
- [x] 窗口管理：半屏、四角、三分屏、最大化、居中、移到下一个显示器（X11、GNOME Wayland、Windows；Windows 上不能移到下一个显示器）
- [x] 表情与符号选择器：中英文搜索、最近使用
- [x] 文件搜索：打开时把主目录的文件名读进内存（约 0.3 秒），按键即模糊匹配；跳过隐藏目录和 node_modules 等
- [x] 结束进程：按名称、命令或监听端口搜索
- [x] 切换音频输入/输出设备
- [x] 浏览器书签与历史搜索（Firefox、Chrome、Chromium、Brave、Edge）
- [ ] **P2** 文件内容搜索（ripgrep）
- [ ] **P2** 浏览器已打开标签页搜索（需要浏览器扩展配合）
- [ ] **P2** 浮动笔记（Floating Notes）
- [ ] **P2** 日程（读取本地日历 / CalDAV）
- [ ] **P2** 取色器、翻译、词典
- [ ] **P2** 专注模式（Focus）

## 快捷链接与片段（后续）

- [x] **P1** 文本片段的关键词自动展开（在任意应用里输入关键词即替换，需要键盘监听）
- [ ] **P2** 快捷链接指定打开方式（Open With）、自定义图标

## 剪贴板历史（后续）

「本地记忆搜索」的调研和分期见 `reports/Heyo 本地记忆搜索调研规划.md`。

- [x] **记忆搜索阶段 0** 历史迁到 SQLite（`history.db`，旧 `history.json` 自动迁移后删除）；中文单字索引 + 三字子串索引；规则解析时间/来源/类型，读不出结果时退回纯文字；上限从 1000 条放宽到 5 万条（主要靠保留期限）；记录「搜了什么、选了哪条」；`heyo --clipboard-eval` 评测
- [x] **记忆搜索：评测工具** `heyo --clipboard-eval --export/--generate`（生成只发给本机模型）、合成的 130 条语料和 168 个用例
- [x] **记忆搜索阶段 1 的实现** 按语义查找（multilingual-e5-small，默认关闭，设置里下载）；合成集上前 5 命中 42% → 93%，查询 p95 约 7 毫秒
- [ ] **P1 记忆搜索：真实评测** 真实历史只有 33 条、没有选择记录，判定标准（四周真实使用里有 ≥10% 的选择只靠语义进前五）还答不了；跨语言需要每种语言约 16 条以上才起作用。原条目： 用真实历史攒到 100 条以上（日常使用的选择记录 + 本地模型按类型合成的查询），这是进入阶段 1 的前提
- [x] **P1 记忆搜索阶段 1** 剪贴板语义检索：multilingual-e5-small（int8）跑在 OCR 用的 onnxruntime-node 里，后台向量队列，暴力检索，与关键词结果融合；是否继续按报告里的四条标准判断
- [ ] **P2 记忆搜索阶段 2–4** 浏览器历史、主动截图的 OCR、文件名进同一个搜索
- [ ] **P2** 拼音/首字母搜索；数据库加密（密钥放 Secret Service）

- [x] 以纯文本粘贴（Paste as Plain Text 命令，可绑快捷键；历史记录本身只存纯文本）
- [ ] **P2** 粘贴后保持窗口
- [x] **P1** 忽略指定应用（如密码管理器以外的敏感应用）
- [x] **P1** 显示来源应用图标
- [ ] **P2** 图片 OCR、编辑条目后再粘贴
- [ ] **P2** Wayland 下的变更监听（目前回退为每秒轮询文本）

## 选中文字与截图

- [x] 选中文字操作：翻译、润色、纠错、缩短、解释、总结（Claude，流式），搜索；结果可替换原文
- [x] 截图：框选、矩形/箭头/画笔标注、复制、保存、贴图、取字（本地 PaddleOCR）
- [x] AI 操作支持本地模型（OpenAI 兼容接口：vLLM、LM Studio、Ollama；与语音润色共用）；`scripts/local-model.sh` 用 Docker 启动 vLLM + Qwen
- [x] 截图：文字标注、打码、移动和调整选区；标注可拖动
- [ ] **P2** 截图：双击文字标注重新编辑；标注改颜色、删除单个
- [x] 自定义选中文字操作（自己写提示词）
- [x] **P1** 截图：窗口自动吸附
- [ ] **P2** 长截图、录屏 / GIF、多显示器同时框选

## 语音输入

- [x] 快捷键开始/结束录音，本地识别（SenseVoice），边说边在浮窗出字，停顿后逐句粘贴
- [x] 可选的 Claude 润色（整理 / 翻译成英文）
- [x] 按住说话、松开结束（短按则是按一下开始、再按一下结束；仅 X11）
- [x] 单键按住说话（右 Alt、左 Alt、右 Ctrl 等）：监听原始按键，不占用该键，短按和组合键不触发
- [x] **P1** 自定义词汇表（人名、术语），提高专有名词准确率
- [ ] **P2** 更多润色模式和自定义提示词；按当前应用自动选择风格
- [ ] **P2** 可选的更高精度模型或云端识别

## AI

- [x] Quick AI：在根搜索输入问题，按 Tab 交给 AI 回答（Claude 或本地模型），可追问
- [x] **P1** AI Chat 窗口
- [ ] **P2** AI Commands：对选中文本执行「改写、翻译、总结」等提示词
- [ ] **P2** 扩展的 `useAI`（`AI.ask` 已可用）；`AI.ask` 的 `creativity` 映射到温度

## 扩展与 Raycast API

- [x] 从 Raycast Store 搜索、安装、卸载扩展
- [x] List / Grid / Detail / Form / ActionPanel / 导航 / Toast / Alert
- [x] Clipboard、LocalStorage、Cache、偏好设置、命令参数、`@raycast/utils`
- [x] 设置里启用/停用命令、编辑扩展偏好
- [x] 扩展更新：Manage Extensions 检测商店新版本，单个或全部更新
- [x] 后台/定时命令（`interval`，`LaunchType.Background`）：运行过一次后按间隔自动执行
- [x] 菜单栏命令（MenuBarExtra）→ 系统托盘：标题、图标、分组、子菜单、点击动作
- [x] **P1** 菜单栏点击提速：菜单渲染完不卸载命令，点击时直接调用已有的动作函数（现在要重新加载并重新取数据，延迟等于一次网络请求）。前提：后台进程里「当前命令」的全局状态改成每个命令各带自己的上下文（如 AsyncLocalStorage），否则常驻的多个命令会串偏好和存储目录
- [ ] **P2** 菜单栏：菜单项图标、Raycast 内置图标作托盘图标、`alternate` 项、从托盘移除的入口
- [x] 扩展的 `AI.ask` 可用（走设置里选的模型）
- [x] OAuth（`OAuth.PKCEClient`）：注册为 `raycast://` 链接的处理程序接住授权码；令牌按扩展保存（权限 600）
- [ ] **P1** 用真实服务（GitHub、Linear 等）逐个验证 OAuth 扩展
- [ ] **P2** 其它 `raycast://` 深链接（打开扩展命令）
- [x] **P1** 开发者模式：导入本地扩展、`ray develop` 热重载、显示日志
- [x] **P1** 跨扩展 `launchCommand`
- [x] `WindowManagement`、`getFrontmostApplication`（主机报告窗口位置并可精确摆放；X11 实测，GNOME Wayland 和 Windows 只过了编译）
- [x] 旧版 API 的别名（`CopyToClipboardAction`、`FormTextField`、`getLocalStorageItem` 等 37 个）、`AI.Model`、`Color.Brown`；旧属性名 `enableFiltering`、`accessoryTitle`、`accessoryIcon`
- [x] 一致性测试：对照真实的 `@raycast/api` 类型，缺的名字和从未读取的组件属性必须登记（`runtime/src/api/conformance.gaps.json`、`props.gaps.json`）
- [x] **P1** 表单的 `storeValue`（9 个组件：记住上次的值）
- [x] **P1** `Form.FilePicker` 的选项（多选、选目录、选文件、显示隐藏文件）
- [x] **P1** `Action.CreateSnippet` / `CreateQuicklink` 接到 Heyo 自己的片段和快捷链接；`Action.PickDate`；`Action.ToggleQuickLook` 和 `quickLook`
- [ ] **P2** `environment.appearance` 跟随真实主题（现在写死为深色）；`environment.launchContext`；`updateCommandMetadata`；`getSelectedFinderItems`；`Form.enableDrafts`、`TextArea.enableMarkdown`；窗口的 `application` 字段
- [ ] **P2** 一致性测试加深到函数签名；名字级缺口里剩下的 16 个（`render()`、不稳定接口、较新的 OAuth 和 `Cache` 常量）
- [ ] **P2** `BrowserExtension`（需要自己发布浏览器扩展）；Raycast 的 AI 扩展（`Tool`）
- [ ] **P2** 脚本命令（Script Commands）：带元数据注释的 shell 脚本作为命令
- [ ] **P2** Deeplink：`heyo://extensions/<author>/<ext>/<command>`
- [ ] **P2** 过滤掉明确只支持 macOS 的扩展，或在商店标注

## 外观与设置

- [x] Raycast 风格的设置窗口（General / Extensions / Advanced / About）
- [x] 开机启动、Node.js 路径
- [x] 浅色主题 / 跟随系统：`src/styles/` 按模块拆分，`tokens.css` 用 `light-dark()` 一处定义明暗两套配色，设置里可选 System/Light/Dark
- [x] 视觉打磨：样式改为 SCSS（scales 映射表 + mixin），4px 间距网格、带行高和字距的字号阶梯、统一的图标块、滑动的选中高亮、骨架屏和空状态、面板投影（窗口留透明边距）、出现/切换/弹出的动效、五态交互
- [ ] **P2** 毛玻璃背景（依赖各平台合成器）；设置页的快捷键表格、听写和截图部分的样式还没用 mixin 重写；Windows 上的字体渲染没看过
- [x] 紧凑模式（只显示搜索栏，输入后展开）；设置窗口自绘边框
- [ ] **P2** 紧凑模式下的收藏栏；设置窗口的缩放和最小化
- [ ] **P2** 字体大小、窗口位置记忆、多显示器在当前屏幕显示
- [ ] **P2** 设置导入/导出

## 代码审查（2026-10-07，五个角度）

第一批已修：Alt+F4 后崩溃、商店安装接受 `..` 作扩展名、设置窗口覆盖面板里改的收藏和别名、设置文件损坏时静默重置、无视图命令结束时误关别的命令、托盘图标删不掉、输入法组字时回车/Esc/Tab 被当成命令、Ctrl+K 执行后键盘失灵、弹层在隐藏后残留。

- [x] **安全** CSP；Markdown 白名单（无 `<style>`、表单、媒体）且最后一步才净化；链接只放行 http/https/mailto；`runtime_send` 只转发白名单消息；API key 不再回传给任何窗口；资源协议屏蔽密钥环、令牌和剪贴板历史
- [x] **隐私** 剪贴板历史跳过 Heyo 自己的管道式写入（语音输入的粘贴与恢复）、扩展标记为 `concealed` 的复制、密码管理器窗口里的复制；目录 0700 / 文件 0600；AI 结果按原文显示（不渲染 HTML、不加载图片），粘贴前去掉控制字符；浏览器数据库的副本放进私有缓存目录；截图残留文件用完即删
- [x] **P1 安全** 按窗口限制可调用的命令（现在所有窗口都能调全部命令）；本地模型地址非本机时提示；按需下载固定版本并校验；粘贴到终端的多行文本先确认
- [ ] **P2 隐私** 剪贴板忽略列表做成设置项、「暂停记录」命令；快捷链接图标不再向 Google 请求
- [x] **P0 后台运行时** 每个命令独立上下文或独立 worker；后台无视图命令加超时；菜单项动作执行完再卸载；后台命令不能隐藏/显示面板；`launchCommand` 先校验再关闭、支持后台类型；OAuth 回调也发给后台进程；模块重复加载的泄漏
- [x] **P1 输入** 扩展控制的搜索框和表单值改成本地优先（现在每个字都等运行时回传，会丢字、打断输入法）；从子视图返回时恢复搜索词和选中项；根搜索刷新时保持选中项
- [x] **地基** 拆 `lib.rs`（1082 → 260 行）；桌面相关代码收进 `platform/`，X11 实现在 `platform/x11/`，带会话检测和能力表
- [x] **地基（2026-10-10 架构评审的 8 项）** 设置只声明一次；平台后端只选一次（`Desktop` 特征）；三层消息只定义一次（`wire.rs` → `shared/bindings/`）；路径集中（`paths.rs`）；子进程行协议（`helper.rs`）；系统命令归主机；列表选中逻辑（`selection.ts`）；内置命令目录一份、按 id 打开一条路径（`open.rs`）、排序测试。Rust 测试 84 个，前端与运行时测试 11 个（vitest）
- [x] 崩溃钩子、Heyo 自己的日志文件、「复制诊断信息」
- [ ] **P1 地基** `scripts/install.sh` 和二进制/辅助脚本的版本校验；计算器和占位符的测试；切换音频设备、结束进程仍在运行时里直接调 `pactl`/`ps`/`ss`
- [x] **Wayland（GNOME）第一步** `gnome-extension/heyo@heyo.app` + `platform/gnome/`：窗口列表/切换/关闭/布局、粘贴、选中文字、剪贴板监听与读写、抓屏、把自己的窗口带到前台；只应答 Heyo 进程；在 Docker 的 GNOME 50 会话里验证
- [x] **Wayland（GNOME）第二步** 按住说话（扩展把单个修饰键注册为快捷键报告按下，轮询修饰键状态得知松开，用空闲时间判断中途是否按了别的键）；设置里检测并一键安装扩展（扩展文件内嵌在二进制里）
- [x] **Windows 能启动** 用条件编译隔开 Linux 专用代码，应用列表来自开始菜单快捷方式；从 Linux 交叉编译（llvm-mingw），在 Docker 里的 Windows 11 ARM 虚拟机上启动并能打开应用（`scripts/windows-test/`）
- [x] **Windows 后端第一批**（`platform/win/`）全局快捷键（默认 Alt+Space，存 `hotkeys.json`）、窗口列表/切换/布局、粘贴、剪贴板监听；`heyo.exe` 旁的 `node.exe` 直接用；数据在 `%APPDATA%\heyo`、缓存和日志在 `%LOCALAPPDATA%\heyo`；在虚拟机里实测：快捷键唤出、左半屏布局、表情粘贴到终端、剪贴板历史记到来源应用
- [x] **Windows 后端第二批（只编译过，没运行）** 截屏、选中文字（UI Automation，回退 Ctrl+C）、按住说话（低级键盘钩子）、.lnk 解析与应用图标、商店应用、跨显示器布局、开机自启、安装器；核对清单在 `scripts/windows-test/CHECKLIST.md`
- [ ] **P1 Windows 余下**（上一条全部需要在虚拟机里实测）听写的录音（建议 `heyo.exe --record` 走 WASAPI）、语音和识别引擎是否有 win-arm64 版本、截图浮层的原生底图、片段展开、音频设备和进程列表。原列表： 系统命令、打开文件、回收站的 Windows 实现已写但没在虚拟机里跑过；截图和 OCR、选中文字、按住说话和听写（模型下载、麦克风）、应用图标（从 .lnk 取）、按应用切换窗口（.lnk 指向的程序名）、UWP/商店应用、安装包和开机自启、多显示器、Heyo 自己粘贴的内容不该记进历史；Windows 上的编译警告
- [x] **Wayland（GNOME）第三步** 扩展有了协议版本，设置里区分"缺失/过期/待生效/已生效"；按阅读核对了 GNOME 45–50 的接口；缩放和多显示器下的截取与布局（容器里四种布局 110 项检查）；Heyo 的窗口不进 Dash/切换器/概览；锁屏后按住说话不丢；`docs/wayland.md` 写了 KDE、wlroots、门户的方案
- [ ] **P1 Wayland 余下** 还没做的：GNOME 46 真机实测；提交到 extensions.gnome.org（缺 `metadata.json` 的 url，需要人来传）；Heyo 先于 Shell 启用扩展时会退回通用适配器直到重启；KDE/其他桌面未实现。原列表： 扩展升级后的提示（现在版本不匹配会显示为未安装，重新安装即可）；GNOME 46（Ubuntu 24.04）的 Wayland 会话实测；高分屏/多显示器下的截图坐标；让 Heyo 的窗口不出现在 Dock 和概览里；提交到 extensions.gnome.org；KDE/其他桌面
- [x] 主线程上的阻塞调用改异步；`Cache` 多实例互相覆盖；59 个长序列表情显示成破图
- [ ] **P2** `appPicker` 偏好；列表虚拟化；快捷键设置的原子性

## 扩展 API（`@heyo/api`）

规格在 `docs/extension-api.md`（草案 v0.2）：`@raycast/api` 的超集，换一个 import 即可迁移，另加 Heyo 自有能力。

- [ ] **待定** 规格第 12 节的 9 个问题：进程隔离是否进 v1、无清单扩展的窗口权限、根搜索提供者的同意方式、包名、`Memory.add`、「仅这一次」、类型转导出还是自有、是否支持 `Tool`、`BrowserExtension`
- [x] **P1 阶段 A** `ext.*` 请求、带类型码的错误、请求带扩展名
- [x] **P1 阶段 B** 权限：清单、判定、询问框、存储、设置页
- [x] **P1 阶段 C** `@heyo/api` 包（两个包名指向同一实现）；`heyo build` / `migrate` / `ext check`
- [x] **P1 阶段 D** v1 模块：Windows、Screen、Dictation、Memory、Files、System、AI
- [x] **P1 阶段 E** 内置命令改用同一套接口，删掉对应的 `host.*`
- [ ] **P2 阶段 F–G** 事件通道和窗口事件；根搜索结果提供者
- [ ] **P2 阶段 H** 每个扩展一个进程
- [ ] **搁置** 在 macOS 的 Raycast 里验证迁移后的扩展（这台 ARM 机器跑不了 macOS；需要真 Mac 或云端 Mac）

## 发行

- [x] 安装包能构建：deb、rpm、AppImage、Windows NSIS 安装器；`scripts/install.sh`；程序与辅助脚本的版本校验；更新器已接入但默认不编译（`packaging/README.md`）
- [ ] **P0 发行余下（需要人来做）** 许可证（仓库没有，包里也就没有）；安装并运行每种包；x86_64 构建；更新用的签名密钥和发布地址；Windows 代码签名；AUR 和 Flatpak 清单没有构建过（Flatpak 的结论是按现状不可行）
- [x] **P1** Raycast 扩展兼容度看板：自动跑热门扩展，公开哪些能用
- [ ] **P2** 从 Raycast、Ulauncher、Albert 导入；首次引导
- [ ] **P2** 「Heyo」的商标查重

## 性能

- [x] 窗口阴影改为图片（软件渲染下每次重绘都在重算模糊）；选中高亮在列表边缘不再回跳；`scripts/gui-test/scroll.sh` 测量
- [ ] **P2** 真机上的按键导航帧数没有实测（沙箱里每步绘制 11–13 毫秒）

## 界面引擎

- [x] **试过 Flutter，搁置**（2026-10-11）：渲染快三到四倍、输入法可用，但要重写整个界面层、画不了扩展里的 HTML 和 SVG、稳定版没有多窗口、Windows 不能交叉编译。数据和理由在 `docs/flutter-evaluation.md`，代码在分支 `flutter-ui`
- [x] **渲染性能：查清了原因**（2026-10-11，`research_notes/gpu/webkitgtk.md`）：NVIDIA 的 GBM 后端需要 `nvidia-drm modeset=1`，而 DGX Spark 的镜像用 `nvidia-drm-options-modeset0` 包把它关掉了（`/etc/modprobe.d/zz-nvidia-drm-override.conf`），WebKit 因此退到 Mesa 的软件路径并失败。现在的 CPU 路径关掉了全部加速合成（没有 `backdrop-filter` 和 3D）。新增设置 Advanced → Rendering：Automatic / CPU / GPU 共享内存 / GPU DMA-BUF
- [ ] **P1 渲染性能：两条出路** (a) 去掉那个包或覆盖文件，启用 `nvidia-drm.modeset=1`，重启并冷启动一次（论坛上同型号机器可行；需要用户决定）；(b) 过渡期用"GPU 共享内存"：显卡绘制但每帧固定约 36 ms（UI 的 FrameDone 握手串行化加同步回读，2.52 没有开关能改），2× 缩放下比 CPU 的 65 ms 好、1× 下比 16 ms 差。替代内核（Verso 已归档、CEF 运行时是 400 MB 的 alpha、Electron 124 MB）都不值得
- [ ] **P1 显示缩放**：开发机的桌面在 2026-10-11 变成了 2×（`Xft.dpi` 192），CPU 路径下每帧 65 ms；待确认是否有意为之
- [x] `environment.appearance` 跟随设置（启动时经环境变量，改变时经消息；选"跟随系统"时在启动和改设置时读一次系统主题，系统主题中途变化不跟随）

## 这一轮留下的（2026-10-10 并行实现 P0/P1 之后）

- [ ] **P1 实测** 大量功能只有单元测试、没在界面里看过：后台常驻的菜单栏命令、输入本地优先、返回恢复、`storeValue`、文件选择器、日期选择、Quick Look、AI 对话（没调过真实模型）、剪贴板忽略列表和暂停、听写词汇、终端粘贴确认、诊断信息、按语义查找的界面、扩展 API 的听写和带图提问、设置里的权限列表
- [ ] **P1 片段展开的手工检查** 真实键盘、Electron/Qt/Firefox、终端、输入法、死键；GNOME Wayland 和 Windows 不支持
- [x] **扩展 API 阶段 E** 切换窗口、系统命令、音频设备、文件搜索、剪贴板历史的读取和复制改走 `@heyo/api`，删掉 15 个 `host.*`；Ask AI 和 AI Chat 没转（`AI.ask` 不支持对话历史）
- [x] **规格修订（阶段 E 的结论）** 已做成 v0.3；`AI.ask` 支持 `messages`、`system`、`images`、`material`，Ask AI / AI Chat / 选中文字动作都改走它。原条目： `MemoryItem` 的字段、`Memory.search` 的按语义查找、去掉 `ClipboardHistory.search`、`AI.ask` 支持历史消息、`Windows.list()` 的顺序约定、上限 200 还是 300、`System` 是否包含音频和进程
- [x] **P2 扩展 API 与规格的偏差** 拒绝只记 60 秒；`offline` 恒为假；`apiVersion` 不拦安装；从 Manage Extensions 卸载不清权限；安装时不显示权限
- [ ] **P1 决定** 表单密码框的 `storeValue` 现在会明文存（仅本人可读）；AI 回答在面板关闭后继续生成
- [ ] **P2** 后台进程里类组件生命周期、ref 回调读到的"当前命令"可能是别的命令；`Cache` 跨进程读改写没有锁；只有菜单栏命令的扩展无法被询问跨扩展启动
- [ ] **P2** `heyo dev` 重建后菜单栏和后台命令不重启；AI 工具（`tools/*.ts`）不构建
- [ ] **P2** 从 Heyo 自己的命令粘贴时，剪贴板条目的来源应用有时记成 Heyo、有时记成目标窗口（取决于监听器处理时谁在前台）
- [ ] **P2** 听写辅助进程用音频文件测试时丢了开头第一个词（"Front center" → "Center."），原因未查
- [ ] **P2** Node 的 `readline` 会在 U+2028/U+2029 处断行，`runtime/src/protocol.ts` 可能受影响
- [x] **实时活动第一步**（2026-10-11）：主机里的活动表（序号保护、上限、两步降级、排序）、`ext.activity.*` 与权限 `activity`、`@heyo/api` 的 `LiveActivity`、面板里的"进行中"区域、内置 Timer 命令、通知作为唯一呈现器。偏差：新增 `elapsed` 事件；`handles` 写在命令项上；每个扩展的开关存在设置里
- [ ] **实时活动（灵动岛）** 方案 `docs/live-activities.md` v0.3（调研 + 讨论结论：状态栏为主阵地并承载菜单栏扩展、折叠态收数据而展开态可自定义界面、面板里的"进行中"区域、默认安静）。待你定第 12 节的问题后做阶段 0 原型；"进行中"区域不依赖任何决定，可以先做
- [ ] 听写窗口在沙箱里有过三次「出现和消失都滞后」，之后 16 次未复现，原因未查实

- [ ] 大列表首次渲染较大（约 1900 个带动作的条目 ≈ 4.5 MB），可按需加载动作面板

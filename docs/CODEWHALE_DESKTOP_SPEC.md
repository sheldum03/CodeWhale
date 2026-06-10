# CodeWhale Windows 桌面端规格说明

> 状态：草案
> 产品名：CodeWhale
> 目标平台：Windows 桌面端
> 默认语言：简体中文，可切换英文
> 设计依据：`C:\Users\enigma\Desktop\ai-project\DUI\docs\design-md\design.md`

## 1. 产品目标

CodeWhale 桌面端是一个 Windows 原生桌面工作台，用图形界面承载 CodeWhale 的本地智能体能力。目标体验接近 Codex app：用户可以选择本地项目，创建或恢复线程，查看流式回复，审查文件和 Git 变更，批准敏感操作，并最终通过正式安装包交付。

桌面端必须保持本地优先。CodeWhale 后端运行在用户机器上，运行时 API 默认绑定 `localhost`，模型密钥和配置存储在本地。

## 2. 已确认决策

| 事项 | 决策 |
|---|---|
| 产品名称 | CodeWhale |
| 首发平台 | Windows 桌面端 |
| 最终交付 | 正式产品级安装包 |
| 架构方向 | 桌面壳 + 本地 CodeWhale 运行时 |
| 开发阶段后端 | 可依赖本机已安装的 `codewhale.cmd` |
| 发布阶段后端 | 将 CodeWhale 后端二进制作为 sidecar 打包 |
| 模型协议 | 只支持 DeepSeek / OpenAI-compatible |
| 默认语言 | 简体中文 |
| 英文支持 | 支持 UI 切换英文 |
| 密钥存储第一版 | 本地 `.env` 文件 |
| 视觉风格 | 遵循 DeepSeek Design Language |

## 3. 初始非目标

- 第一版不做 macOS / Linux 桌面端。
- 第一版不做云端托管任务或公网 relay。
- 第一版不暴露运行时 API 到公网。
- 第一版不做 provider 市场，只提供 DeepSeek 和 OpenAI-compatible 配置。
- 第一版不做企业级策略管理。
- 第一版暂不接入 Windows Credential Manager，后续再迁移。

## 4. 推荐架构

### 4.1 技术栈

推荐新增独立目录：

```text
desktop/
  package.json
  src/
  src-tauri/
  public/
  .env.example
```

推荐使用：

- Tauri：Windows 桌面壳、进程管理、文件系统能力。
- React + TypeScript：前端应用。
- Tailwind CSS：设计 token 与布局。
- Rust/Tauri command：启动、停止、监控 CodeWhale 后端进程。

不要直接复用 `web/` 作为桌面端主体。当前 `web/` 是社区/官网站点，不是本地智能体工作台。

### 4.2 本地运行时

桌面端负责启动并监督本地 CodeWhale 运行时：

```powershell
codewhale serve --http --host 127.0.0.1 --port 7878 --auth-token <token>
```

桌面端启动流程：

1. 读取或生成 runtime token。
2. 优先连接已有兼容运行时。
3. 如果运行时不存在，则启动本地后端。
4. 轮询 `GET /health`。
5. 读取 `codewhale doctor --json` 和 `GET /v1/runtime/info`。
6. 所有 `/v1/*` 请求携带 `Authorization: Bearer <token>`。
7. 通过 SSE 连接 `/v1/threads/{id}/events?since_seq=<seq>`。
8. 应用退出时默认停止由桌面端启动的后端进程。

### 4.3 发布形态

开发阶段允许调用全局命令：

```powershell
codewhale.cmd serve --http
```

正式安装包应内置 sidecar：

```text
desktop/src-tauri/binaries/
  codewhale-x86_64-pc-windows-msvc.exe
  codewhale-tui-x86_64-pc-windows-msvc.exe
```

打包后的应用优先使用内置 sidecar。只有开发模式或诊断模式才回退到系统 PATH 中的 `codewhale.cmd`。

## 5. 运行时 API 范围

现有 `docs/RUNTIME_API.md` 是后端集成契约来源。桌面端第一阶段优先接入以下接口：

| 能力 | API |
|---|---|
| 健康检查 | `GET /health` |
| 运行时信息 | `GET /v1/runtime/info` |
| 会话列表 | `GET /v1/sessions` |
| 恢复旧会话 | `POST /v1/sessions/{id}/resume-thread` |
| 创建线程 | `POST /v1/threads` |
| 线程列表 | `GET /v1/threads` |
| 线程摘要 | `GET /v1/threads/summary` |
| 读取线程 | `GET /v1/threads/{id}` |
| 更新线程 | `PATCH /v1/threads/{id}` |
| 恢复线程 | `POST /v1/threads/{id}/resume` |
| 分叉线程 | `POST /v1/threads/{id}/fork` |
| 发送消息 | `POST /v1/threads/{id}/turns` |
| 追加引导 | `POST /v1/threads/{id}/turns/{turn_id}/steer` |
| 中断任务 | `POST /v1/threads/{id}/turns/{turn_id}/interrupt` |
| 压缩上下文 | `POST /v1/threads/{id}/compact` |
| 审批操作 | `POST /v1/approvals/{approval_id}` |
| 实时事件 | `GET /v1/threads/{id}/events?since_seq=<u64>` |
| 用量统计 | `GET /v1/usage` |

如果开发中发现接口缺失或行为与文档不一致，应先更新 `docs/RUNTIME_API.md`，再调整桌面端实现。

## 6. 配置方案

### 6.1 `.env` 第一版格式

开发阶段建议使用：

```text
desktop/.env
```

示例：

```dotenv
CODEWHALE_PROVIDER=deepseek
CODEWHALE_BASE_URL=https://api.deepseek.com
CODEWHALE_API_KEY=
CODEWHALE_MODEL=deepseek-v4-pro
CODEWHALE_RUNTIME_HOST=127.0.0.1
CODEWHALE_RUNTIME_PORT=7878
CODEWHALE_LANGUAGE=zh-CN
CODEWHALE_THEME=system
```

OpenAI-compatible 示例：

```dotenv
CODEWHALE_PROVIDER=openai-compatible
CODEWHALE_BASE_URL=https://example.com/v1
CODEWHALE_API_KEY=
CODEWHALE_MODEL=
```

正式安装包阶段建议迁移到：

```text
%APPDATA%\CodeWhale\.env
```

### 6.2 设置页

设置页必须包含：

- Provider：DeepSeek / OpenAI-compatible。
- Base URL。
- API key。
- Model。
- 语言：简体中文 / English。
- 主题：跟随系统 / 亮色 / 暗色。
- 默认项目目录。
- Runtime 端口。
- 重启后端。
- 导出诊断信息。

## 7. 信息架构

### 7.1 主要页面

| 页面 | 用途 |
|---|---|
| 欢迎 / 项目选择 | 选择或重新打开项目目录 |
| 工作台 | 线程、对话、运行状态、变更审查 |
| 线程详情 | 完整对话与 turn 生命周期 |
| Git 变更 | 查看 changed files 和 diff |
| 终端 / 日志 | 查看命令输出和后端日志 |
| 设置 | Provider、语言、主题、运行时、诊断 |
| 诊断 | 后端健康、配置路径、端口、sidecar 状态 |

### 7.2 工作台布局

桌面端默认布局：

```text
┌─────────────────────────────────────────────────────────────┐
│ 顶栏：项目名、运行时状态、provider、设置                    │
├──────────────┬──────────────────────────────┬───────────────┤
│ 项目/线程    │ 对话区                       │ 活动/变更     │
│ 会话列表     │ 输入框                       │ Git/日志/审批 │
│ 最近项目     │ 流式输出                     │               │
└──────────────┴──────────────────────────────┴───────────────┘
```

首屏必须是可用的工作台或项目选择界面，不做营销式 landing page。

## 8. 设计系统要求

遵循 `design.md` 中的 DeepSeek 设计风格指南。

### 8.1 设计原则

界面应体现：

- 清晰：信息层次明确，关键状态一眼可见。
- 克制：避免无意义装饰。
- 亲和：圆润但不过度幼态。
- 高效：面向生产力场景，减少重复操作。
- 呼吸感：保留足够留白，避免拥挤。

### 8.2 色彩 token

| Token | 亮色 | 暗色 |
|---|---|---|
| 品牌渐变 | `#4D6BFE` 到 `#7B5CFC` | `#6B8AFF` 到 `#9F7AFF` |
| 主文字 | `#1A1A2E` | `#F0F0F5` |
| 次级文字 | `#6B7280` | `#A0A0B0` |
| 辅助文字 | `#9CA3AF` | `#6B7280` |
| 背景 | `#FFFFFF` | `#121216` |
| 容器 | `#F8F9FC` | `#1E1E26` |
| 分割线 | `#E5E7EB` | `#2D2D38` |
| 成功 | `#10B981` | `#10B981` |
| 警告 | `#F59E0B` | `#F59E0B` |
| 错误 | `#EF4444` | `#EF4444` |
| 信息 | `#3B82F6` | `#3B82F6` |

### 8.3 字体

- 中文：Windows 默认使用 `Microsoft YaHei`。
- 英文/数字：优先 `Inter`。
- 代码、路径、端口、token、日志：优先 `JetBrains Mono`。

字号：

| 层级 | 字号 | 行高 | 字重 |
|---|---:|---:|---:|
| H1 | 28px | 40px | 700 |
| H2 | 22px | 32px | 600 |
| H3 | 18px | 28px | 500 |
| 正文 | 16px | 24px | 400 |
| 辅助 | 14px | 20px | 400 |
| 元信息 | 12px | 18px | 400 |

### 8.4 组件规范

- 主按钮：蓝紫渐变，白字，10px 圆角。
- 次按钮：透明背景，1.5px 渐变描边。
- 输入框：12px 圆角，44px 高度，水平 padding 16px。
- 卡片：16px 圆角，仅用于重复项、工具块、模态内容，不堆叠卡片。
- 弹窗/抽屉：20px 圆角。
- 标签：8px 圆角。
- 用户气泡：右对齐，渐变背景，白字，右下角直角。
- AI 气泡：左对齐，浅灰/深色容器，左上角直角，带细小渐变点缀。
- 代码块：深色终端风格，8px 圆角。

### 8.5 图标与动效

- 图标使用 24x24 线性图标，1.5px 描边，round cap / round join。
- React 端可使用 `lucide-react`，除非后续确定更完整的图标库。
- 微交互：150-200ms。
- 展开/hover：200-300ms。
- 页面切换：300-400ms。
- 思考状态：蓝紫渐变呼吸光效或波浪进度条。
- 流式输出：逐字淡入，2px 光标闪烁。

### 8.6 无障碍

- 普通文本对比度满足 WCAG 2.1 AA。
- 交互目标不小于 44x44px。
- 支持键盘导航。
- 焦点状态使用清晰的 2px 渐变轮廓。
- 暗色模式是一等功能，不做简单反色。

## 9. 国际化

从第一版开始建立文案字典：

```text
desktop/src/i18n/zh-CN.ts
desktop/src/i18n/en-US.ts
```

要求：

- 默认 `zh-CN`。
- 所有按钮、菜单、错误提示、空状态、设置项、审批弹窗都必须走字典。
- 语言切换尽量即时生效；如果实现成本高，允许重载后生效。

## 10. 安全与权限

### 10.1 运行时边界

- 后端默认绑定 `127.0.0.1`。
- `/v1/*` 必须带 bearer token。
- token 不出现在可见日志或诊断导出中。
- 默认拒绝连接非本机运行时地址，除非用户在高级设置中显式启用。

### 10.2 项目信任

首次打开项目目录时显示信任提示：

- 信任：允许读取项目文件，并在需要写文件、执行命令、联网时请求审批。
- 不信任：只做只读浏览和诊断。

信任记录按绝对路径保存在本地。

### 10.3 审批体验

桌面端必须展示运行时发出的 approval 请求，并调用：

```text
POST /v1/approvals/{approval_id}
```

审批弹窗包含：

- 操作摘要。
- 命令或文件路径。
- 工作目录。
- 风险级别。
- 允许一次。
- 拒绝。
- 如果后端支持，提供“本项目记住此选择”。

### 10.4 密钥处理

第一版：

- API key 写入 `.env`。
- `.env` 必须加入 `.gitignore`。
- 设置页提示用户不要提交 `.env`。

后续：

- 迁移到 Windows Credential Manager。
- 诊断导出自动脱敏。
- 日志中隐藏 API key、token、Authorization header。

## 11. P0-P4 开发计划

## P0：架构与 API 对齐

目标：建立桌面端骨架，确认本地运行时可被稳定管理。

任务：

- 创建 `desktop/` 工程。
- 接入 Tauri、React、TypeScript、Tailwind。
- 建立 i18n 字典。
- 添加 `.env.example`。
- 实现运行时 supervisor 抽象。
- 开发阶段支持启动 `codewhale.cmd serve --http`。
- 实现 runtime token 生成与注入。
- 实现 `GET /health` 轮询。
- 实现 `GET /v1/runtime/info` 读取。
- 实现 `codewhale doctor --json` 读取。
- 从 `design.md` 提取设计 token。
- 记录后端接口差异或缺口。

验收：

- Windows 上可打开桌面端窗口。
- 应用可启动或连接本地 CodeWhale 后端。
- 能显示运行时健康状态。
- 能显示 provider、model、runtime 基础信息。
- P0 不要求真实模型调用。

## P1：最小可用桌面端

目标：跑通核心智能体闭环。

任务：

- 项目选择页。
- 最近项目列表。
- 项目信任提示。
- 主工作台布局。
- 线程列表：`GET /v1/threads`。
- 创建线程：`POST /v1/threads`。
- 发送消息：`POST /v1/threads/{id}/turns`。
- 监听 SSE：`GET /v1/threads/{id}/events?since_seq=0`。
- 渲染流式输出。
- 中断任务：`POST /v1/threads/{id}/turns/{turn_id}/interrupt`。
- 恢复线程：`POST /v1/threads/{id}/resume`。
- 恢复旧会话：`POST /v1/sessions/{id}/resume-thread`。
- 设置页支持 DeepSeek / OpenAI-compatible。
- 保存语言、主题、端口、最近项目。

验收：

- 用户能选择项目并创建线程。
- 用户能发送 prompt 并看到流式输出。
- 用户能停止当前任务。
- 用户重开应用后能恢复最近线程。
- MVP 主路径中文文案完整。

## P2：接近 Codex app 的工作台体验

目标：让桌面端能够承担真实开发任务。

任务：

- 支持多个线程同时运行或监听。
- 线程状态：排队、运行中、完成、失败、已中断、已归档。
- 线程重命名和归档：`PATCH /v1/threads/{id}`。
- 线程分叉：`POST /v1/threads/{id}/fork`。
- 活动面板：事件、命令、工具调用、审批。
- 审批弹窗和审批历史。
- Git 面板：
  - 当前分支。
  - changed files。
  - diff viewer。
  - 打开文件。
- 后端日志面板。
- 诊断页：
  - runtime health。
  - doctor output。
  - 配置路径。
  - runtime 端口。
  - sidecar 路径。
- 用量面板：`GET /v1/usage`。

验收：

- 多线程切换不会丢失事件状态。
- 审批请求可在 UI 中处理。
- 用户能审查模型造成的文件变更。
- 后端启动失败、端口占用、密钥无效时有清晰解决方案。

## P3：产品体验完善

目标：把工具变成可长期使用的桌面产品。

任务：

- 内置终端面板。
- 本地网页预览，识别 localhost dev server。
- 会话搜索与筛选。
- 系统通知：任务完成、失败、需要审批。
- 快捷键：
  - 新建线程。
  - 停止当前任务。
  - 切换线程。
  - 打开设置。
  - 打开命令面板。
- 命令面板。
- 插件/技能只读浏览。
- Git worktree 隔离任务。
- 首次启动向导：
  - 选择语言。
  - 配置 provider。
  - 选择项目。
  - 运行健康检查。
- 空状态、加载状态、错误状态插图。
- 英文文案补齐。

验收：

- 新用户不阅读 CLI 文档也能完成首次配置。
- 常见工作流可发现、可键盘操作。
- 中英文 UI 覆盖所有可见文案。
- 亮色和暗色模式都可用。

## P4：正式发布

目标：发布正式 Windows 桌面安装包。

任务：

- 打包 CodeWhale 后端 sidecar。
- 构建 Windows 安装器。
- 添加应用图标、产品名、版本信息。
- 添加关于页。
- 添加更新策略：
  - 第一阶段手动检查更新。
  - 后续在签名和发布源稳定后做自动更新。
- 崩溃日志。
- 诊断信息导出。
- 干净 Windows 机器安装测试。
- 安装、升级、卸载测试矩阵。
- 发布 checklist。
- 用户文档：
  - 安装。
  - 配置 DeepSeek。
  - 配置 OpenAI-compatible endpoint。
  - 选择项目。
  - 运行第一个线程。
  - 后端启动失败排查。
  - 端口占用恢复。
- 打包流程稳定后接入 CI。

验收：

- 干净 Windows 机器可安装并运行 CodeWhale。
- 用户不需要手动安装 `codewhale.cmd`。
- 打包应用能启动内置后端。
- 配置 provider 后能完成 P1 主路径。
- 安装器、图标、版本、卸载行为正确。

## 12. 里程碑

| 里程碑 | 范围 | 产物 |
|---|---|---|
| M0 | P0 | 桌面端骨架、运行时健康检查 |
| M1 | P1 核心 | 项目选择、创建线程、流式聊天 |
| M2 | P1 完整 | 设置、恢复、中断、中文 MVP |
| M3 | P2 | 多线程、审批、Git 变更、诊断 |
| M4 | P3 | 终端、预览、快捷键、引导、英文 |
| M5 | P4 | 安装包、sidecar、发布文档、干净机器验收 |

## 13. 测试策略

### 13.1 单元测试

- runtime client 请求构造。
- SSE event parser。
- i18n 字典完整性。
- 设置项校验。
- 项目信任存储。
- provider 配置校验。

### 13.2 集成测试

- 启动 runtime 进程。
- 轮询 health。
- 创建线程。
- 发送消息。
- 接收 SSE。
- 中断 turn。
- 提交 approval。
- runtime 崩溃后恢复。

### 13.3 UI 测试

- 项目选择。
- 首次启动向导。
- 设置保存和重载。
- 聊天流式渲染。
- 审批弹窗。
- 主题切换。
- 语言切换。
- 小窗口和大窗口布局。

### 13.4 发布测试

- 干净 Windows 安装。
- 从开始菜单启动。
- sidecar 路径解析。
- `.env` 创建和读取。
- 端口 `7878` 被占用时自动提示或换端口。
- 卸载时保留用户数据，除非用户显式选择清除。

## 14. 待确认问题

- 正式版 `.env` 是否固定放在 `%APPDATA%\CodeWhale\.env`？
- 关闭窗口后，后端是否继续运行？
- 是否支持导入已有 `~/.codewhale/config.toml`？
- 产品图标使用现有 `assets/` 中的哪一个？
- DeepSeek 是否作为默认 provider，OpenAI-compatible 放在高级选项？

## 15. 下一步

1. 确认正式版 `.env` 存储位置。
2. 创建 `desktop/` Tauri 工程。
3. 建立设计 token 和中英文 i18n。
4. 实现 runtime supervisor 和 `GET /health`。
5. 做出第一版项目选择页和工作台外壳。

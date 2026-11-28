<div align="center">

<img src="assets/moca_icon.png" alt="Moca Logo" width="112" />

# Moca

**M**odular **O**rchestration & **C**loud **A**utomation

基于 **Rust (Edition 2024) + MaaFramework + egui** 构建的多游戏通用自动化底座，已率先接入 **《蓝色星原：旅谣》（Azur Promilia）**。  
支持 **蓝色星原实机自动跳过剧情、一键批量兑换礼包码（内置公测码 + OCR 结果回传）、日常/清体力/家园托管**、Schema 驱动多游戏接入与 **Win32 端游 / ADB 模拟器**双端控制。

[![Latest Release](https://img.shields.io/github/v/release/limitcool/moca?label=Windows%20x64%20下载&color=3b82f6)](https://github.com/limitcool/moca/releases/latest)
[![License](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-Edition%202024-orange.svg)](https://www.rust-lang.org/)
[![MaaFramework](https://img.shields.io/badge/Powered%20by-MaaFramework%20v5.16-emerald.svg)](https://github.com/MaaXYZ/MaaFramework)

> **萌新直接下载（Windows 10/11 x64，免配置环境）**：  
> 👉 [**点此一键下载最新版便携包 `Moca-windows-x86_64.zip`**](https://github.com/limitcool/moca/releases/latest/download/Moca-windows-x86_64.zip) ｜ [查看所有 Releases 版本与更新说明](https://github.com/limitcool/moca/releases/latest)  
> 解压后直接双击 **`Moca.exe`** 即可使用（已内置 MaaFramework SDK、PP-OCRv5 中文识别模型、自动申请管理员权限与《蓝色星原》剧情跳过 / 批量兑换码配置，无需安装 Rust）。

<img src="docs/preview.png" alt="Moca Desktop UI Preview" width="900" />

</div>

---

## 功能列表

| 功能模块 | 触发 / 运行方式 | 状态 | 核心能力与说明 |
| :--- | :--- | :--- | :--- |
| **自动跳过剧情** | 实时触发开关 | ✅ 实机可用 | 零硬性 `sleep` 动态变速状态机（非剧情 `1000ms` 低功耗，命中 `>>` 或弹窗切 `60~80ms` + `inverse` 消失等待）；识别右上 `>>`、剧情梗概「跳过剧情」与二次确认框；支持四档策略热重载：`全自动`（全跳）/ `跳梗概`（跳过梗概、确认框停住）/ `点确认`（自动确认、梗概停住）/ `仅箭头`（仅点 `>>`）。 |
| **批量兑换礼包码** | 一次性快捷任务 | ✅ 实机可用 | 内置《蓝色星原》公测全套 13 个兑换码，支持多行增删粘贴与一键重置；自动导航 `Esc → 设置 → 账户 → 兑换`，Maa 原生 PP-OCRv5 识别输入框与弹窗文字，逐条回传兑换结果并在末尾汇总退出；执行时自动暂停并恢复剧情跳过，防误点。 |
| **全局热键与前台避让** | 系统级辅助 | ✅ 实机可用 | 标准 Win32 `RegisterHotKey`（无键盘钩子）：`F8` 暂停/恢复实时辅助、`F9` 急停全部任务、`F10` 显隐窗口，窗口内 `F5` 截图/刷新窗口；前台独占模式下切回 Moca 自动静默避让焦点，不强拉游戏。 |
| **应用内自动更新与节点优选** | 启动静默 / 手动触发 | ✅ 实机可用 | 启动自动检测新版本，支持轻量热更包与完整包自动切换、SHA-256 校验与保留 `profiles/` 一键重启；内置国内 GitHub 加速代理（`gh-proxy` / `ghfast` / `ghproxy`）并发竞速与 `128KB` 样本本机测速优选，并预留自定义镜像与客户端 Cloudflare 自动优选。 |
| **双端接入与状态工作台** | 底座通用能力 | ✅ 实机可用 | 支持 Win32 端游（正则自动匹配窗口、进程图标、UIPI 提权校验、后台并行/前台独占切换）与 ADB 模拟器；按游戏 TOML 持久化（`profiles/<game_id>.toml`），内置 **ROI 采图框选工具**一键导出模板 PNG。 |
| **日常托管批处理** | 队列批处理 | 🚧 适配中（骨架就绪） | 启动与每日签到（含邮箱附件）、日常清体力刷本（副本与轮次可选）、家园收菜与奇波喂养、每日活跃与纪行领奖（实机模板持续补充中）。 |

---

## 核心特性

- **多游戏共用同一套底座（Game-Agnostic）**
  - 底座零硬编码任何具体游戏逻辑，完整兼容 [MaaFramework ProjectInterface v2](https://github.com/MaaXYZ/MaaFramework)（`interface.json`）并轻量扩展权限标记。
  - 新增一款游戏只需在 `games/<game_id>/` 放入 `interface.json` 与 `resource/`，底座自动生成任务列表、Toggle 开关与参数表单，按游戏独立持久化配置到 `profiles/<game_id>.toml`。
- **Win32 端游 + ADB 模拟器双端一体**
  - **Win32**：支持窗口探测、按 `title_regex` 智能匹配目标游戏窗口、UIPI 管理员权限隔离检测、后台截图 / 前台独占焦点互斥锁。
  - **ADB**：自动发现本地 Android 模拟器（MuMu 等）并调用 MaaFramework 极速截图与触控。
- **极致轻量的 Fluent 桌面端（支持“一戏一皮肤”）**
  - 基于 `egui (eframe)` + DX12 后端，结合 **Win11 Fluent 层叠质感与 HIG 一体化分段控件**，无 WebView / Node 运行时负担，内存占用低、画面预览零延迟；支持每个游戏包通过 `interface.json` 自定义专属皮肤主题。
  - **内置 ROI 采图框选工具**：直接在软件画布上拉框获取 `[x, y, w, h]` 坐标并一键裁剪保存为 `resource/image/*.png` 模板，大幅降低新游戏和版本更新适配成本。
- **应用内自动更新与本机节点优选**
  - **启动静默检测 & 一键热更**：启动后静默检测新版本，标题栏实时提示；支持根据本地 `MaaFramework` 版本与 OCR 模型情况自动切换**轻量热更包**或**完整包**，校验 SHA-256 后保留 `profiles/` 用户配置一键重启更新。
  - **本机实时测速优选**：内置国内 GitHub 加速代理（`gh-proxy` / `ghfast` / `ghproxy`）与 GitHub 直连并发竞速，支持 128KB 样本真实吞吐测速排名，并可按需接入自定义镜像与客户端本地 Cloudflare 节点自动优选。
- **Open-Core 清晰边界**
  - 日常签到、清体力、基础刷图与图像识别配置全部**开源免费**在本地执行，方便社区共建与热修；
  - 复杂动态规划/高难策略通过独立抽象层（`moca-cloud`）按需对接云端决策能力。

---

## 游戏适配进度

| 游戏 | 游戏包 ID | 支持端 | 当前状态 |
| :--- | :--- | :--- | :--- |
| **蓝色星原：旅谣** (*Azur Promilia*) | `azur-promilia` | Win32 / ADB | 🚧 **适配进行中**：已完成游戏包接入、任务 Schema、双端控制声明、**零硬性等待剧情自动跳过**、**PP-OCRv5 批量兑换礼包码**与 TOML 配置持久化，持续补齐其余实机日常模板与流程节点。 |

> **说明**：《蓝色星原：旅谣》已率先完成**自动跳过剧情**与**批量兑换礼包码（含实机 OCR 结果回传）**全流程接入；日常、清体力与家园流程骨架均已就绪，欢迎使用底座自带的 **ROI 采图框选工具** 一起补充实机图像模板与流程 PR！详情见 [CONTRIBUTING.md](CONTRIBUTING.md)。

---

## 快速上手

### 方式一：普通玩家开箱即用（推荐）

1. 前往 [**GitHub Latest Release**](https://github.com/limitcool/moca/releases/latest) 或 [**直接下载 `Moca-windows-x86_64.zip`**](https://github.com/limitcool/moca/releases/latest/download/Moca-windows-x86_64.zip)。
2. 将压缩包完整解压到任意全英文/常规文件夹（保留 `Moca.exe` 同级的 `games/`、`*.dll` 与 `plugins/`）。
3. 双击运行 **`Moca.exe`**（程序会自动申请管理员权限以适配游戏；若跳过提权可在「前台独占」或无 UIPI 限制的窗口下使用）。

### 方式二：开发者源码构建

#### 1. 环境准备

- [Rust 工具链](https://rustup.rs/)（支持 `edition = "2024"`）
- Windows 10/11（默认 DX12 渲染，可通过 `WGPU_BACKEND` 环境变量覆盖）
- **MaaFramework SDK**：从 [MaaFramework Releases](https://github.com/MaaXYZ/MaaFramework/releases) 下载对应平台的 SDK（如 `MAA-win-x86_64-v5.16.1.zip`），解压到以下任一位置：
  - 仓库根目录下的 `vendor/maa-sdk/`（确保存在 `vendor/maa-sdk/bin/MaaFramework.dll`）
  - 或者设置环境变量 `MAA_SDK_PATH` 指向解压后的 SDK 目录

#### 2. 启动运行与测试

```powershell
# 启动 Moca 桌面底座
cargo run -p moca-ui

# 运行全部单元测试（含 MaaFramework 动态加载与资源节点校验）
cargo test --workspace
```

---

## 工程架构

```text
moca/
├── .github/workflows/   # Windows x64 打 Tag 自动构建与 Release 工作流
├── assets/              # 应用图标与静态资源
├── crates/
│   ├── moca-protocol/   # ProjectInterface v2 兼容协议、pipeline_override、TOML 持久化
│   ├── moca-maa/        # MaaFramework 动态加载、Win32 图标提取/探测、UIPI 检测、Tasker 会话
│   ├── moca-runtime/    # tokio 异步运行时、多实例焦点互斥锁、游戏包扫描器
│   ├── moca-cloud/      # 策略接口抽象层
│   └── moca-ui/         # 无边框 egui 桌面端、Schema 表单渲染与热更新、ROI 采图框选工具
├── games/
│   └── azur-promilia/   # 第一款接入游戏：《蓝色星原：旅谣》
└── docs/                # 界面截图与文档资源
```

---

## 路线图 (Roadmap)

- [x] **Phase 1**：通用多游戏底座骨架（Rust Edition 2024 + `MaaFramework` 动态加载 + 无边框 `egui` 工作台）
- [x] **Phase 2**：接入《蓝色星原：旅谣》游戏包、**实机零延迟自动跳过剧情**（支持四档弹窗策略热切换）、**Maa PP-OCRv5 批量兑换礼包码与结果回传**、Win32 全局热键（`F8`/`F9`/`F10`/`F5`）与前台焦点避让、TOML 配置持久化、ROI 采图框选工具、Windows x64 Release CI
- [ ] **Phase 3**：公测开服后完善《蓝色星原：旅谣》实机日常/清体力/家园模板与流程
- [ ] **Phase 4**：资源包在线检查更新、多账号轮询队列与云端策略链路对接

---

## 致谢与声明

- 核心图像识别与控制能力基于开源项目 [MaaFramework](https://github.com/MaaXYZ/MaaFramework)（LGPL-3.0）构建。
- 本仓库代码采用 [Apache-2.0](LICENSE) 协议开源。
- 本项目仅供学习交流与研究自动化技术原理使用，请遵守各游戏厂商的用户协议并合理使用。

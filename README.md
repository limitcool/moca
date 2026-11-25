<div align="center">

# Moca

**M**odular **O**rchestration & **C**loud **A**utomation

基于 **Rust (Edition 2024) + MaaFramework + egui** 构建的多游戏通用自动化底座，已率先接入 **《蓝色星原：旅谣》（Azur Promilia）**。  
支持 **蓝色星原日常/清体力/家园托管**、Schema 驱动多游戏接入与 **Win32 端游 / ADB 模拟器**双端控制。

[![License](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-Edition%202024-orange.svg)](https://www.rust-lang.org/)
[![MaaFramework](https://img.shields.io/badge/Powered%20by-MaaFramework%20v5.16-emerald.svg)](https://github.com/MaaXYZ/MaaFramework)

<img src="docs/preview.png" alt="Moca Desktop UI Preview" width="900" />

</div>

---

## 核心特性

- **多游戏共用同一套底座（Game-Agnostic）**
  - 底座零硬编码任何具体游戏逻辑，完整兼容 [MaaFramework ProjectInterface v2](https://github.com/MaaXYZ/MaaFramework)（`interface.json`）并轻量扩展权限标记。
  - 新增一款游戏只需在 `games/<game_id>/` 放入 `interface.json` 与 `resource/`，底座自动生成任务列表、Toggle 开关与参数表单，按游戏独立持久化配置到 `profiles/<game_id>.json`。
- **Win32 端游 + ADB 模拟器双端一体**
  - **Win32**：支持窗口探测、按 `title_regex` 智能匹配目标游戏窗口、UIPI 管理员权限隔离检测、后台截图 / 前台独占焦点互斥锁。
  - **ADB**：自动发现本地 Android 模拟器（MuMu 等）并调用 MaaFramework 极速截图与触控。
- **极致轻量的无边框纯 Rust 桌面端**
  - 基于 `egui (eframe)` + DX12 后端，无 WebView / Node 运行时负担，内存占用低、画面预览零延迟。
  - **内置 ROI 采图框选工具**：直接在软件画布上拉框获取 `[x, y, w, h]` 坐标并一键裁剪保存为 `resource/image/*.png` 模板，大幅降低新游戏和版本更新适配成本。
- **Open-Core 清晰边界**
  - 日常签到、清体力、基础刷图与图像识别配置全部**开源免费**在本地执行，方便社区共建与热修；
  - 复杂动态规划/高难策略通过独立抽象层（`moca-cloud`）按需对接云端决策能力。

---

## 游戏适配进度

| 游戏 | 游戏包 ID | 支持端 | 当前状态 |
| :--- | :--- | :--- | :--- |
| **蓝色星原：旅谣** (*Azur Promilia*) | `azur-promilia` | Win32 / ADB | 🚧 **开服适配中**：已完成游戏包接入、任务 Schema、双端控制声明与 Pipeline 骨架；将于 11 月 26 日公测开服后持续填充实机图像模板与流程节点。 |
| 示例通用包 (*Moca Demo*) | `moca-demo` | Win32 / ADB | ✅ 用于验证 Schema 动态表单与权限隔离逻辑。 |

> **说明**：目前《蓝色星原：旅谣》处于公测前夕（`resource/pipeline/*.json` 为占位骨架节点），欢迎有兴趣的开发者在开服后使用底座自带的 **ROI 采图框选工具** 一起提交模板与日常流程 PR！详情见 [CONTRIBUTING.md](CONTRIBUTING.md)。

---

## 快速上手

### 1. 环境准备

- [Rust 工具链](https://rustup.rs/)（支持 `edition = "2024"`）
- Windows 10/11（默认 DX12 渲染，可通过 `WGPU_BACKEND` 环境变量覆盖）
- **MaaFramework SDK**：从 [MaaFramework Releases](https://github.com/MaaXYZ/MaaFramework/releases) 下载对应平台的 SDK（如 `MAA-win-x86_64-v5.16.1.zip`），解压到以下任一位置：
  - 仓库根目录下的 `vendor/maa-sdk/`（确保存在 `vendor/maa-sdk/bin/MaaFramework.dll`）
  - 或者设置环境变量 `MAA_SDK_PATH` 指向解压后的 SDK 目录

### 2. 启动运行与测试

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
├── crates/
│   ├── moca-protocol/   # ProjectInterface v2 兼容协议、Profile 持久化、Cloud DTO
│   ├── moca-maa/        # MaaFramework 动态加载、ADB/Win32 探测、UIPI 检测、Tasker 会话封装
│   ├── moca-runtime/    # tokio 异步运行时、多实例焦点互斥锁、游戏包扫描器
│   ├── moca-cloud/      # VIP 鉴权与云端策略接口抽象
│   └── moca-ui/         # 无边框 egui 桌面端、Schema 表单渲染器、ROI 采图框选工具
├── games/
│   ├── azur-promilia/   # 第一款接入游戏：《蓝色星原：旅谣》
│   └── moca-demo/       # 通用 Schema 示例包
└── docs/                # 界面截图与文档资源
```

---

## 路线图 (Roadmap)

- [x] **Phase 1**：通用多游戏底座骨架（Rust Edition 2024 + `MaaFramework` 动态加载 + 无边框 `egui` 工作台）
- [x] **Phase 2**：接入《蓝色星原：旅谣》游戏包、真实 `Tasker` 异步执行与停止、Win32 智能窗口匹配、Profile 持久化、ROI 采图框选工具
- [ ] **Phase 3**：公测开服后完善《蓝色星原：旅谣》实机日常/清体力/家园模板与流程
- [ ] **Phase 4**：资源包在线检查更新、多账号轮询队列与云端策略链路对接

---

## 致谢与声明

- 核心图像识别与控制能力基于开源项目 [MaaFramework](https://github.com/MaaXYZ/MaaFramework)（LGPL-3.0）构建。
- 本仓库代码采用 [Apache-2.0](LICENSE) 协议开源。
- 本项目仅供学习交流与研究自动化技术原理使用，请遵守各游戏厂商的用户协议并合理使用。

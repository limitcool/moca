# 贡献指南 (Contributing to Moca)

感谢你对 **Moca** 的关注！我们非常欢迎社区贡献游戏适配包（`games/<game_id>/`）、Pipeline 节点修复与底座功能改进。

## 为《蓝色星原：旅谣》或新游戏补充模板与流程

1. 启动桌面端底座并连接目标游戏窗口 / ADB 模拟器：
   ```powershell
   cargo run -p moca-ui
   ```
2. 点击右上角 **「ROI 采图框选」**，在实时画面上拖拽框选按钮或区域：
   - 点击 **「复制坐标」** 获取 `[x, y, w, h]` ROI 数组；
   - 输入模板名并点击 **「保存模板 PNG」**，将自动裁剪导出到 `games/<game_id>/resource/image/<name>.png`。
3. 在 `games/<game_id>/resource/pipeline/*.json` 中编写或补充对应的 MaaFramework 识别与动作节点。
4. 提交 PR 前请确认本地测试通过：
   ```powershell
   cargo test --workspace
   ```
   （`moca_maa` 中的测试会自动加载 `MaaFramework` 验证游戏资源包的 JSON 节点是否合法。）

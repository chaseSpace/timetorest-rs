# 一叶 · LeafShield

简体中文 | [English](README.en.md) · **作者：Leigg**

基于 Rust 和 Tauri 2 的桌面休息提醒与屏幕调节工具。通过定时休息、全屏壁纸和需要手动输入的正念文案，减少连续盯屏的时间。

当前版本 **0.6.1**，提供 Windows x64 便携版、安装包及 macOS 通用 DMG（Apple Silicon + Intel）。软件与文档均支持中英文；在“选项 → 语言”选择 `English` 或 `简体中文`，切换后立即生效并保存。

## 下载与运行

从 [Releases](https://github.com/chaseSpace/timetorest-rs/releases/latest) 下载：

| 文件                             | 用途                           |
| -------------------------------- | ------------------------------ |
| `leafshield-0.6.1.exe`           | 便携版，放在固定目录后直接运行 |
| `LeafShield_0.6.1_x64-setup.exe` | Windows 当前用户安装包         |
| `SHA256SUMS.txt`                 | 程序和许可文件的 SHA256 校验值 |
| `LICENSE`                        | MIT 许可文件                   |

macOS 下载：`LeafShield_0.6.1_universal.dmg`，校验文件为 `SHA256SUMS-macos.txt`，同时校验 DMG 和 LICENSE。

Windows 需要 Microsoft Edge WebView2 Runtime。正式程序的界面资源已嵌入安装包，运行时不启动 HTTP 服务，不需要 Node.js 或 Vite。Windows 发行文件未做代码签名；macOS 应用使用 ad-hoc 签名，未提供 Developer ID 签名或公证。

1. 运行程序，设置工作和休息时长，默认 **45 分钟工作 / 2 分钟休息**。
2. 到点后，每个显示器显示独立的置顶休息页，倒计时结束自动返回。
3. 提前结束时，移动鼠标或按键唤出解锁区域，逐字输入所选文案。
4. 关闭主窗口会收起到系统托盘，计时继续；从托盘重新打开窗口或退出。

**Windows 便携版也默认开启当前用户开机启动**，可在“选项 → 开机启动”关闭。登录启动时主界面隐藏在托盘。移动或重命名 EXE 后手动运行一次，会根据保存的偏好更新启动路径，无需管理员权限。

macOS 打开 DMG 后将 LeafShield 拖入 Applications，首次打开按系统提示授权。键盘防护需在“系统设置 → 隐私与安全性 → 辅助功能”授权后重启。ad-hoc 签名限制见 [Tauri macOS 签名说明](https://v2.tauri.app/distribute/sign/macos/)。

## 功能

切换语言时，设置页、锁屏、悬浮窗、托盘和提醒一起切换；五条解锁文案也切换为对应语言，并保留所选序号。界面截图展示中文版。

| 功能             | 行为                                                                                                   |
| ---------------- | ------------------------------------------------------------------------------------------------------ |
| 计时             | 快捷组合 `25/2`、`45/2`、`60/3`、`90/5`；工作 1–240 分钟，休息 1–60 分钟；支持暂停、继续和立即休息     |
| 提前提醒         | 开启后，在休息前 60 秒和 30 秒发送系统通知及桌面轻量提醒                                               |
| 多屏休息页       | 默认隐藏解锁区域和鼠标；实际移动鼠标或按键后唤出，输入框自动聚焦；失去焦点后重新隐藏                   |
| 正念解锁         | 固定五条预设；提示文案不可选中或复制；输入框禁止粘贴和拖入；输错显示红色下划线，中文组合输入期间不误报 |
| 可选紧急通道     | 20 秒内手动按键 50 次；间隔超过 2 秒重置，长按不计数，成功解锁记录日志                                 |
| 悬浮计时         | 四边吸附、离开后收起、靠近展开；支持鼠标穿透和恢复位置，按工作区与 DPI 计算边界                        |
| 前台白名单       | 最多五个进程文件名；支持选择 EXE 或桌面 `.lnk`；仅匹配程序处于前台时延后休息                           |
| Windows 屏幕调节 | 色温 1500–6500K、亮度 20–100%；默认 5000K / 80%，拖动预览、松开保存                                    |
| 休息背景         | 林间、远山、星夜三张离线照片，每张最多 300 KB；支持导入自定义图片                                      |
| 本机记录         | 按日统计专注与休息时长、休息次数，保留最近 200 条日志；单实例运行                                      |

自定义背景接受 JPEG / PNG / WebP，原文件最多 5 MB、1600 万像素、任一边不超过 8192 像素。导入后校正方向，缩小至 1920×1200 内，移除原图元数据，存为不超过 500 KB 的 JPEG；只保留当前自定义文件。

## 界面预览

| 显示器                                                           | 计时器                                                             |
| ---------------------------------------------------------------- | ------------------------------------------------------------------ |
| ![色温与亮度](.github/images/屏幕截图%202025-12-20%20170703.png) | ![工作与休息计时](.github/images/Snipaste_2026-11-02_13-30-19.png) |

| 解锁文案                                                         | 关于                                                         |
| ---------------------------------------------------------------- | ------------------------------------------------------------ |
| ![解锁文案预设](.github/images/Snipaste_2026-11-02_13-30-28.png) | ![关于一叶](.github/images/Snipaste_2026-11-02_13-30-38.png) |

## 技术栈

| 层           | 技术与职责                                                                            |
| ------------ | ------------------------------------------------------------------------------------- |
| 桌面容器     | **Tauri 2**：系统 WebView、多窗口、托盘、通知、打包、单实例                           |
| 原生后端     | **Rust 2024 edition**：状态机、系统接口、图片处理、持久化和参数校验                   |
| 界面         | **TypeScript + HTML + CSS**，使用 **Vite** 构建，无 React / Vue 运行时                |
| 数据和图片   | `serde` / `serde_json`、`chrono`、`image`、`sha2`；本地 JSON 和文件，无数据库或服务端 |
| Windows 集成 | Win32 / Windows COM：系统颜色矩阵、键盘钩子、前台进程、快捷方式解析、当前用户启动项   |
| 验证和发布   | Rust 测试、rustfmt / Clippy、Playwright、GitHub Actions                               |

## 从源码构建

需要 Node.js **22.12+**、Rust stable 和对应平台的 Tauri 原生依赖。Windows 另需 MSVC C++ Build Tools、Windows SDK 和 WebView2。

```powershell
git clone git@github.com:chaseSpace/timetorest-rs.git
cd timetorest-rs
npm ci
npm run build             # 便携版 → artifacts/leafshield-0.6.1.exe
npm run build:installer   # Windows NSIS 安装包
```

构建校验值写入 `artifacts/SHA256SUMS.txt`。`artifacts/leafshield.exe` 是本地运行副本；旧副本被占用时保留旧文件，请运行新的带版本号文件。生成安装包不会自动安装。

Windows 构建脚本支持 `CARGO_HOME`、`RUSTUP_HOME`、`CARGO_TARGET_DIR`，并为旧终端补读系统级 Rust 环境。设置 `CARGO_HOME` 时，默认缓存使用其下的 `targets/leafshield`。项目 [`.cargo/config.toml`](.cargo/config.toml) 使用 RsProxy，删除镜像配置即可直连 crates.io；托管发布 runner 使用 crates.io。

```powershell
npm run build:web         # TypeScript 检查和静态资源构建
npm run test:rust         # Rust 工作区测试
npm run check:rust        # rustfmt / Clippy
npm run test:ui           # Playwright 直接拦截请求读取 dist，无 HTTP 服务
npm run smoke             # 原生启动 / IPC 检查，需先构建并退出已有实例
```

Windows UI 测试使用系统 Edge，其他系统先执行 `npx playwright install chromium`。[原生检查脚本](scripts/) 可能移动鼠标、打开预览或临时调整设置，不应与手动验收同时运行。自动化检查不能替代真实双屏、输入法和休息流程验收。

只有显式执行 `npm run desktop` / `npm run dev` 时，才启动本机 Vite 开发服务 `127.0.0.1:1420`。

### 平台状态

| 平台          | 当前范围                                                                                                                |
| ------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Windows x64   | 本次发布平台，已完成本机发布编译及部分原生验证；最新双屏交互仍由用户手动验收                                            |
| macOS         | 通用 DMG 支持 Apple Silicon 和 Intel；CI 检查双架构和签名完整性。键盘防护需辅助功能授权，真机交互仍需手动验收。         |
| Linux X11     | 提供源码适配和 CI 配置；依赖 GTK / WebKitGTK / AppIndicator / X11，前台检测使用 `xprop`；本次未提供安装包或完成真机验收 |
| Linux Wayland | 全局键盘、前台检测和置顶行为受合成器限制，不保证与 X11 一致                                                             |

Ubuntu / Debian 依赖示例：

```sh
sudo apt-get install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev libayatana-appindicator3-dev libx11-dev x11-utils
```

## 发布

更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`Cargo.lock`、`src-tauri/tauri.conf.json` 和界面版本号，在 `docs/releases/` 添加版本说明后推送 `v<版本号>` 标签。[发布工作流](.github/workflows/release.yml) 校验版本，构建 Windows 便携版和 NSIS 安装包，为程序和许可文件生成校验值，全部上传完成后公开 Release。失败的上传保留为草稿。

[macOS 工作流](.github/workflows/macos-release.yml) 从原应用标签编译 Universal DMG，检查双架构、签名和内置 MIT 许可，在 Windows 发布完成后追加 DMG 及 `SHA256SUMS-macos.txt` 到同一 Release。已有版本可通过推送 `mac-v0.6.1` 为 `v0.6.1` 补发 Mac 安装包。

## 使用边界与资料

这是自愿使用的休息工具，不能阻止 Ctrl+Alt+Del、安全桌面、操作系统级注销或强制结束进程；软件调色也不保证覆盖安全桌面和独占全屏。整体内存需要计入系统 WebView 子进程，不能只看 Rust 宿主占用。

- [默认背景图片来源与许可](public/wallpapers/SOURCES.md)，摄影作品许可独立于应用代码。

## 许可

项目代码采用 [MIT License](LICENSE)，可按许可条款使用、修改和分发。默认背景照片仍遵循各自许可，详见[图片来源](public/wallpapers/SOURCES.md)。

## 鸣谢

感谢 **CareUEyes**。本项目的界面设计和部分功能参考了该软件。

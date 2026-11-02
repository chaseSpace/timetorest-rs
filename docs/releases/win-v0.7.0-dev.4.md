## Windows 原生预览版

这是使用 **Rust + Win32** 重写界面的首个公开预览版本，不依赖 Tauri、WebView2 或 JavaScript 运行时。目前仅支持 Windows x64，建议使用 Windows 11；暂不支持 macOS 和 Linux。

主要功能包括定时休息、系统 RGB 色温与亮度调节、多屏休息页、五条正念解锁文案、紧急解锁、悬浮倒计时、系统托盘、前台程序白名单、中英文界面和自定义休息背景。

本版本通过 39 项 Rust 测试和 18 项 Windows 原生界面自动检查。真实双屏、输入法、系统调色恢复和开机登录流程仍建议手动验证。

本机常驻状态测得专用内存低于 3 MiB；GitHub CI 生成的 Windows x64 便携 EXE 低于 2 MiB。总工作集包含系统共享页，不属于同一统计口径。

下载 `leafshield-native-0.7.0-dev.4.exe` 后可直接运行。程序未进行代码签名，Windows 可能显示安全提示；请使用同一发行中的 `SHA256SUMS.txt` 核对文件。

作者：Leigg（磊哥哥）

许可：MIT
鸣谢：界面设计和部分功能参考 CareUEyes。

//! 桌面可执行入口。
//!
//! 真正的启动逻辑在库里的 `lumino::run`（桌面 bin 与 Android cdylib 共用）；
//! Android 入口是同库的 `android_main`，由 android-activity 的
//! `ANativeActivity_onCreate` 调用，本文件不参与 Android 构建。
//!
//! Windows 子系统控制（由 build.rs 根据 DEBUG env var 自动发射 cfg 标记）：
//!   release profile（DEBUG=false, windows_gui_subsystem 标记激活）→ windows 子系统，隐藏终端
//!   debug / fast-release（DEBUG=true, 无标记）→ console 子系统，显示终端
#![cfg_attr(
    all(target_os = "windows", windows_gui_subsystem),
    windows_subsystem = "windows"
)]

fn main() -> anyhow::Result<()> {
    lumino::run(None)
}

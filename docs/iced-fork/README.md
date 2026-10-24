# iced 0.14 fork 补丁（仓库内可复现副本）

本目录保存 **`PenguinBMDevs/iced` fork 相对上游的全部改动**，以 git patch 形式入库，
使「云端 fork 的某个 rev」与「仓库内可审计的补丁」一一对应。
便于：审查改了什么、在 fork 不可用时本地复现、以及升级上游后重新生成。

> 说明：仓库 `.gitignore` 忽略了 `patches/`（约定该目录只放临时补丁），
> 因此本副本落在**已跟踪**的 `docs/iced-fork/`。**权威来源仍是云端 fork 的分支提交**，
> 本目录是它的可审计镜像。

## 对应关系

| 项 | 值 |
|---|---|
| 上游基线 | `iced-rs/iced` 0.14 分支 `595af03`（= crates.io 已发布 `iced_* 0.14.x` 源码） |
| fork 仓库 | <https://github.com/PenguinBMDevs/iced> |
| fork 分支 | `lumino-android-0.14` |
| fork rev | `5f5139389a48e751b8ddfde9353c742624abd321`（下称 `5f51393`） |
| 补丁文件 | [`0001-android-support.patch`](./0001-android-support.patch)（12.3 KB，4 文件，+127 −12） |

`Cargo.toml` 里 **12 个 iced crate 必须同 rev 一起 patch**：`iced_winit` / `iced_wgpu` 等
包内部是 path 依赖，只改一个会导致同 crate 出现两个 source，Cargo 直接报错。

## 补丁包含三组改动

1. **`winit/src/conversion.rs`** — Android 键盘转换
   winit 的 `platform::modifier_supplement` 只在 windows/macos/x11/wayland/orbital 下编译，
   Android 被排除；上游 `key_without_modifiers()` / `text_with_all_modifiers()` 在 Android
   上 `E0432 unresolved import`，`iced_winit` 整体编不过。
   Android 分支改用 `logical_key`，text 缺失时回退 `Key::Character`。

2. **`graphics/src/text.rs`** — Android 系统字体
   fontdb 0.23 的 `load_system_fonts()` 对 Android 显式跳过（cfg 排除），字体库只剩内嵌字体，
   中文全是豆腐块。改为显式 `load_fonts_dir("/system/fonts")` 并补齐 sans/serif/mono 默认族名。

3. **`wgpu/{lib,text}.rs`** — 段落强引用钉 + 诊断日志
   原 `vendor/iced_wgpu` 的修复迁入：段落弱引用若在渲染线程 `prepare` 前死亡，
   **整段文字会静默不绘制**（菜单栏文字悬停闪烁）。同时迁入 `LUMINO_TEXT_TRACE` 诊断日志。

> **等价性已核验**：`vendor/iced_wgpu/src` 的 37 个文件与本 fork 的 `wgpu/src`
> **逐字节一致**（`lib.rs` / `text.rs` MD5 相同，`git diff --stat` 无差异），
> 只有 `Cargo.toml` 因工作区依赖写法不同而不同。
> ⇒ 把 `vendor/iced_wgpu` 的消费方切到本 fork 属于**行为等价替换**，另加 Android 支持。

## 应用补丁（从零复现 fork 分支）

```bash
git clone https://github.com/PenguinBMDevs/iced.git
cd iced
git checkout -B lumino-android-0.14 595af03
git am /path/to/docs/iced-fork/0001-android-support.patch
git push origin lumino-android-0.14
```

## 重新生成补丁（fork 有新提交时）

```bash
# 在 fork 的本地克隆里
git format-patch 595af03..<新 rev> --stdout --no-signature \
  > /path/to/repo/docs/iced-fork/0001-android-support.patch
```

然后**同步更新 `Cargo.toml` 里 12 处 `rev =`**（保持完全一致）与 `Cargo.lock`。

## 消费方清单

- 桌面 / Android 统一走 fork：`Cargo.toml` `[workspace.dependencies]` 中 12 个 `iced*` 均为
  `git = "https://github.com/PenguinBMDevs/iced.git", rev = "5f51393…"`。
- 历史形态（已废弃）：`iced_wgpu = { path = "vendor/iced_wgpu" }`——该 vendor 副本已随
  本次迁移删除，其内容现等价存在于 fork 的 `wgpu/` 目录。

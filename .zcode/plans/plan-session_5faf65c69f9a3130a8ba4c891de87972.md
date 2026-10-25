## 自动保存功能（最小侵入版）

用户已确认：独立自动保存文件 + 启动弹窗恢复 + 默认关闭。总原则：**自动保存对现有功能零修改、只做增量挂接**——不改保存/关闭/保存确认（SaveConfirm）任何现有逻辑。

### 复用的现有能力（只读，不改动）

- 组包：`build_lmpj_project`（`src/runner/menu/file/save.rs:278`）
- 原子落盘：`save_to_archive`（`crates/core/project/src/project/save.rs:137`，内置临时文件→换入协议）
- 脏标记：`ui().is_project_modified()`（`crates/editor/ui/src/host/dialog/state.rs:21`）
- 定时唤醒模式：`control_flow.rs` 的 deadline 合并（照 `gpu_check.next_wake` 先例）

### Phase A · 设置项（默认关闭，间隔默认 5 分钟）

1. `crates/core/core/src/storage/config.rs` + `config/defaults.rs`：加 `autosave_enabled: bool`（`#[serde(default)]`）与 `autosave_interval_mins: u32`（缺省 5，读取时 clamp 1..=120）。旧配置文件天然兼容。
2. `crates/editor/ui-core/src/settings_event.rs`：加 2 个事件变体。
3. `crates/editor/ui-settings`：`EditingSettings` 加字段、`panel.rs` 回填+match、`pages/editing.rs` 加「自动保存」section（开关+间隔输入，照现有行样式）。
4. i18n 中英文 key（`crates/tools/extras/src/i18n/settings.rs` + translations）。
5. `src/runner/inner/persist.rs`：`config_diff` 与 `patch` 各加 2 处（此管道的关键点，漏了不落盘）。

### Phase B · 周期自动保存（并入现有 `auto_save.rs`，52→约 200 行）

6. 纯决策函数 `autosave_due()`（开关/脏/保存中/到点四条件）+ `autosave_path()`（配置目录 `autosave/autosave.lmpj`），可脱离 winit 单测。
7. 到点后照 `handle_save_file` 的 tokio 异步任务模式写盘，不阻塞 UI；`saving` 置位期间跳过本轮。
8. 挂点仅 3 处增量：`RunnerInner` 加 1 个字段（`autosave_next: Option<Instant>`）；`control_flow.rs` 加 1 段 deadline 合并（照 GPU 检测先例）；`handler.rs` 的 `about_to_wait` 在 `save_storage()` 旁加 1 个 `autosave_tick()` 调用。

### Phase C · 启动恢复（对现有流程零改动）

9. 清理策略收敛为**单一时点**：仅在**正常退出**时删除自动保存文件（用户已明确选择保存或放弃，此时该文件失去意义）；崩溃/强杀时文件留存。不在保存/加载/关闭流程加任何清理钩子。
10. `DialogType` 加 `AutosaveRecover` 变体 + 新增对话框 view（恢复/丢弃两键，文案含文件修改时间）+ `DialogResult::AutosaveRecover(bool)` 一个变体、路由 match 加一个分支——全部为枚举增量，不碰现有对话框逻辑。
11. 选「恢复」→ 复用 load.rs 现有工程加载事件流载入该文件；选「丢弃」→ 删除文件。两动作都在新代码路径里。

### 测试与验证

- 单测（测试代码允许 `expect("上下文")`）：`autosave_due` 决策表、间隔 clamp、配置默认值与旧配置反序列化兼容。
- 按 crate 跑验证（本地全量 `cargo test` 已知挂起，不跑）：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p lumino-core -p lumino-ui-settings -p lumino-ui`。
- 按项目惯例在 `docs/` 写实施记录、更新 CHANGELOG 0.2.1 段落。

### 纪律

全程遵守 AGENTS.md：中文注释、生产代码零 unwrap/expect（用 `?` + thiserror 风格 Result）、文件 ≤400 行、不新增 `mod.rs`。默认不做 git 提交；如需提交按 `feat(settings)` / `feat(runner)` 拆两笔 Conventional Commits。
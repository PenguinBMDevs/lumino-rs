# Changelog

All notable changes to the Lumino project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.1] - Unreleased

### 加载保护 · 内存检查 / 强杀看门狗设置开关（默认关闭）

- **新增设置** — 设置面板「常规 → 加载保护」两个开关，**默认均关闭**：
  - 「加载期间检查内存」(`UiConfig::load_memory_check_enabled`)：加载期间执行同步内存检查，
    RSS 达到软限（总内存 - 512MB）的 95% 时主动终止；同时门控后台监控线程在加载期间的检查。
  - 「加载期间进程强杀看门狗」(`UiConfig::load_watchdog_enabled`)：独立线程在加载期间
    每 50ms 检查，超软限或系统可用内存 < 350MB 时 SIGKILL / TerminateProcess。
- **口径** — 关闭时加载期间不做阈值判定、不主动终止（系统 OOM 仍可能结束进程）；非加载期间
  后台监控保持原行为。开关即时生效：后台/看门狗每轮读标志，加载中途关闭立即停检。
- **安全** — 加载期检查触发时走 `std::process::abort()`（不是 `panic!`）：加载在 `tokio::spawn`
  任务里，panic 会被任务边界吞掉（进程存活且加载标志卡死）。加载状态改用 RAII 守卫
  `MidiLoadGuard`，正常/提前返回/取消/panic 展开都会清除标志。
- **实现** — 新增 `lumino-diagnostics::memory_monitor::load_guard`（进程级原子开关，默认 false）；
  `MemoryMonitor::check_loading()` 供加载链路使用（渲染路径仍走 `check()`，不受开关影响）；
  `spawn_watchdog()` 开关关闭时惰性不建线程，加载入口/设置保存时按需补起。加载入口收敛为
  `RunnerInner::begin_midi_load()`（刷新开关 + 看门狗 + RAII 守卫）。macOS 看门狗本就整体禁用，
  UI 置灰并把配置归一化为关闭。
- **验证** — `cargo fmt --check`、`git diff --check`、`cargo check -p lumino-rs --all-targets`、
  `cargo clippy --workspace --all-targets -- -D warnings` 全部通过；定向测试
  `lumino-diagnostics` 16 / `lumino-core` 211 / `lumino-ui-settings` 26 / `lumino-extras` 30 /
  `lumino-midi-loader` 50 / `lumino-midi-model` 124 / `lumino-rs` 181，全部 0 failed。

### CI 修复 · ubuntu lint（事件循环 API）与 BOM 守卫红灯

- **ubuntu lint（E0599）** — Linux 专用分支把 `with_user_event()` 误挂在 `EventLoopBuilder<T>` 上
  （winit 0.30 里它属于 `EventLoop<T>`）。该分支被 `#[cfg(target_os = "linux")]` 剥离 ⇒ Windows 本机
  永不参与类型检查，只有 ubuntu CI 能炸。现收敛为**不带 cfg** 的 `user_event_builder()`，本机任意平台
  `cargo check` 均可拦截，并补一条编译期契约测试。**存量问题**（`a047e613`，2026-10-16），非 S0 引入。
- **BOM 守卫（5 个 .rs）** — 逐字节剥离文件头 3 字节（`EF BB BF`），其余内容零改动：校验
  `HEAD blob[3:] == 工作区` 逐字节相同、`git diff --numstat` 每文件 1/1。**存量问题**
  （`6dc5f82b`/`dc658893`，2026-10-17），非 S0 引入。
- **预防** — 新增 `.githooks/pre-push`（推送前跑字节级 BOM 守卫，零依赖秒级），
  `git config core.hooksPath .githooks` 启用。
- **验证** — 真 Ubuntu 26.04（WSL）跑通 CI 同款命令：`cargo check --workspace`（6m06s，0 error）、
  `cargo clippy --workspace --all-targets -- -D warnings`（3m08s，0 warning）、`bash scripts/check-bom.sh`
  （1565 个 .rs，0 命中）；Windows 侧 `cargo fmt --check`、`cargo clippy -p lumino-rs --all-targets
  -- -D warnings` 均 RC=0。**`cargo test` 不在本地跑**（首个测试进程长时间未退出），交由 CI 三平台执行。

### S0-5 · 形状随工程存取接线与集成夹具

- **接线（4 处）** — 形状真正随工程进出编辑器：
  - **保存**：`runner/menu/file/export.rs` 组工程时 `project.shapes = editor.shapes_snapshot()`；
  - **打开**：`runner/menu/file/load.rs` 读工程后先取 `project.shapes`，再转换 ParsedMidi；
  - **事件**：新增 `menu::file::Event::ProjectShapes(Vec<ShapeModel>)`，加载侧在
    `MidiParsed` **之后**单独发一条（顺序不可反：文档装载路径会清空形状存储）；
  - **清空**：`clear_drawn_shapes()` 同时清 `shape_store`（新文档与旧形状无关）。
- **夹具（3 项）** — `shapes_snapshot → load_shapes` 往返、`k` 保真、**再生成与视口缩放无关**。
- **验证** — `cargo test -p lumino-note-core` 146 / `-p lumino-message` 47 / `-p lumino-ui-editor` 785 /
  `-p lumino-editor-state` 339 / `-p lumino-ui` 507，全部 0 failed；
  `cargo check -p lumino-rs` 与 `cargo clippy`（6 个 crate）均通过、零警告；改动文件 ≤400 行。

### S0-4 · 通道模式与同键重叠消解

- **新增** — `lumino-note-core::channels`：`ChannelMode`（`Raw` / `Single`）、
  `apply_channel_mode`、`resolve_single_channel`、`MIN_NOTE_TICKS`。
- **口径**（对齐 Spiderweb `help/channels.txt`）：
  - `Raw`（As drawn）= 原样保留，**允许同键重叠**（黑乐谱纹理的前提）；
  - `Single` = 同键重叠消解：同 tick 合并取「最响力度 + 最长长度」；不同起点重叠时
    前者在后者起点截断、后者延伸到前者原终点（后者更长则保留更长）。
- **边界** — 只按 `(key, tick)` 组织、不同音高互不影响；截断结果不低于 `MIN_NOTE_TICKS`；
  结果与输入顺序无关（内部稳定排序）；**不改通道号**（`Multi channel` 自动分配属后续任务）。
- **验证** — `cargo test -p lumino-note-core` 146 passed（含 12 项新增）；
  `cargo clippy -p lumino-note-core --all-targets` 零警告。

### S0-3 · 形状存储、撤销分组回放与再生成接线

- **新增** — `lumino-editor-state::ShapeStore`（文档级形状存储 + 撤销分组日志 + 音符指纹）与
  `lumino-ui-editor` 的 `shape_ops`（`load_shapes` / `create_shape` / `edit_shape` / `delete_shape` /
  `shapes_snapshot`）。
- **撤销口径（关键）** — 形状**不进** `EditorSnapshot`：音符由文档历史回退，形状参数由 `ShapeStore`
  的同分组日志回退，两者共用同一个历史 `group_id` ⇒ 一次参数提交 = 一次撤销单元。
  另修一处既有事实：Snapshot 历史的 `group_id` 在 **redo 侧会丢失**，故 `ShapeStore` 保留 LIFO 撤销栈，
  redo 无分组时兜底回放（否则会出现「音符回来了、形状参数没回来」）。
- **再生成** — 与曲线工具确认**同源**（`flatten_path` → `stroke_band::outline_notes` → `keep_longest`
  → 长度下限）；旧音符按**值指纹**清理，杜绝孤儿音符。
- **协作** — 形状对象不同步（已拍板口径）；它产生的音符按值广播（新增
  `EditorData::push_collab_transform_sync`，与既有 `take_pending_collab_transform_sync` 对称）。
- **验证** — `cargo test -p lumino-editor-state` 339 passed / `-p lumino-ui-editor` 782 passed
  （含 21 项 S0-3 新增，「改参数 → undo → 形状与音符同时回退」为验收项）；
  `cargo clippy -p lumino-editor-state -p lumino-ui-editor --all-targets` 零警告；改动文件 ≤400 行。

### S0-2 · 形状工程部件 `.lmshapes`

- **新增** — 工程文件新增独立部件 `data/project/shapes.lmshapes`（魔数 `LMSH` + 版本 1 + bincode + zstd）：
  `LmshapesData`、`FolderPaths::SHAPES_FILE`、`LuminoProject::shapes` 字段；文件夹与单文件归档两种形态都可存取。
- **兼容性（关键）** — 遵循既有「新部件缺失即空」约定：加载侧 `exists()` 守卫，**形状为空时保存侧不写部件**
  ⇒ 老工程保存后字节不变，且**不提升 `format_version`**；含未知部件的工程仍可加载（旧版本读新工程不报错）。
- **落盘纪律** — bincode 为位置编码：`ShapeModel` / `ShapeOp` 增删字段或变体必须同时提升部件版本。
- **顺带** — `save.rs` 574 行超 400 行红线，测试模块外迁为 `save/tests.rs`（纯移动，零逻辑变更）。
- **验证** — `cargo test -p lumino-project` 70 passed / 0 failed（含 5 项 S0-2 新增）；
  `cargo clippy -p lumino-project --all-targets` 零警告；
  `cargo check -p lumino-export -p lumino-editor-state -p lumino-ui-editor -p lumino-ui` 通过（覆盖加字段影响面）；
  `cargo test -p lumino-core` 84 passed（i18n 覆盖率门禁已含新增 2 条文案）；全部改动文件 ≤400 行。

### S0-1 · 形状数据模型（Spiderweb 图形工具移植底座）

- **新增** — `lumino-core` 新模块 `shape`（`shape.rs` + `shape/{geometry,style,model}.rs`，
  共 831 行含测试）：`ShapeModel` / `ShapeKind` / `ShapeGeometry` / `ShapeAnchor` / `ShapeRect` /
  `ShapeStyle` / `ShapeFill` / `ShapeOp`，全部 `serde` 化。
- **目的** — 把「一个形状」从一次性烘焙的几何残留，变成可持久、可回改、可与它生成的音符同组撤销的
  **文档对象**（Spiderweb 图形工具 45 项缺口清单的公共底座，见
  `docs/2026-10-17-Spiderweb图形工具缺口清单与移植规划.md`）。
- **设计决策** —
  1. 不新建 crate：编辑器域类型同域先例（`Tool` / `BrushConfig`）已在 `lumino-core`，
     且该 crate 已依赖 `serde` / `bincode`，故只加模块、不加依赖边；
  2. 几何不分枚举（统一点列 + 可选外接框），再生成管线只留一条代码分支；
  3. **`k`（beats per key）随形状存储**——圆弧/突起/遮罩带的「圆度」按绘制时屏幕比例定义，
     丢失它会让形状跨缩放走形；
  4. `ops` 为**有序修饰链**（照抄 Spiderweb `notes/fx.py` 语义），S0 只有 `Noop` 占位变体，
     真实算子由后续任务逐个追加到末尾；
  5. 校验返回 `Err(InvalidArgument)` 而非 panic（损坏/手改工程不得让加载路径崩）。
- **未改动** — `ProjectMetadata.format_version`、`EditorSnapshot`、`OpKind`、协作协议、`Tool` 枚举、i18n。
- **验证** — `cargo test -p lumino-core shape` 33 passed / 0 failed（新增 32 项）；
  `cargo clippy -p lumino-core --all-targets` 零警告；`cargo fmt` 干净；
  7 个新文件均 ≤400 行（最大 198）。

### UI-018 / #146 · 加载 MIDI 后标尺小节号从 2 起跳（P0）

- **症状** — 加载多轨 MIDI（如 Bad Apple 8.49M，123 轨 / division 960）后，钢琴
  卷帘首屏小节号从 2 开始，看起来像“自动定位到第 2 小节”；实测视口
  `scroll_x = 0`、`start_tick = 0`，不是滚动问题。
- **根因** — 加载器为保住 EXP-005 的逐事件往返等价，全量保留同 tick 的重复拍号
  （多轨 MIDI 常见：两轨各写一个 tick 0 的 4/4，得到
  `time_signatures = [(0,4,4), (0,4,4)]`）。`MeasureIterator::advance_to`
  （`81cdb8b7` O(段数) 重写）的 `segment_index_at` 取最后一个同 tick 段，
  且 `measures_in_segment` 对零长段 `.max(1)` 计 1 小节，导致首个标签
  `number = 2`；旧 `step()` 实现在 target 0 不步进、编号为 1。
- **修复** — 仅在显示层 `MeasureIterator` 用 `Cow` 对拍号段表做同 tick 去重
  （后者覆盖前者，与 `time_signature_at` 的“最后一个 ≤ tick 生效”一致）；
  无重复时借用原表零分配，有重复时才 `Owned`。加载器与文档保持原样，
  EXP-005 逐事件往返不受影响。
- **验证** — 新增 3 个针对性用例（tick 0 重复 / 中途重复 / 同 tick 冲突），
  并把等价性用例扩展到重复/冲突拍号；`cargo test -p lumino-ui-editor`
  769 通过，`cargo clippy -p lumino-ui-editor --all-targets` 零警告。

### UI-018 / #146 · 走带顶部时间标尺 + 纵向卷帘首标签（P1）

- **症状** — 工程走带视图没有小节号文本（WGPU 只画线），长文件无法按小节导航；
  纵向卷帘把 tick 0 的小节号画在键盘区内，网格里首个可见标签变成第 2 小节。
- **修复** —
  - 新增 `ArrangementRulerCanvas`（`ui-editor/src/arrangement/ruler.rs`）：
    28px 顶部标尺，复用 P0 去重后的 `MeasureIterator` 画小节号与刻度线；
    点击/拖拽发 `ArrangementCursorSet` 跳转，滚轮缩放与钢琴卷帘标尺一致。
  - 走带布局改为三列顶部各留 28px（音轨列表/走带区/垂直滚动条），
    新增 `arrangement_geometry` 纯函数收口 `canvas_offset/canvas_size`
    （此前 `collect_viewport_info` 与 `collect_render_data` 各写一份）；
    垂直滚动条宽度由 12 修正为 20（与 `ScrollbarWidget` 实际宽度一致）。
  - `vertical_labels` 的标签统一画在小节线上方（`label_top_for_line`），
    tick 0 的“1”留在网格内。
- **验证** — `cargo test -p lumino-ui-editor` 775 通过（新增 4 个标尺用例 +
  2 个纵向标签用例）；`cargo test -p lumino-ui --lib` 506 通过（新增 3 个
  几何用例）；`cargo clippy -p lumino-ui-editor -p lumino-ui -p lumino-ui-core
  --all-targets` 零警告。按用户要求未跑 app build，真机目视待补。

### AUD-005 返工 · 软件合成后端启动零 WinMM 触碰（#149）

- **症状** — LGS + OmniMIDI v2 环境下启动概率性/必然崩溃（0xc0000005）：初始实现
  （D1-A 决策）为软件后端保留 System 占位输出（WinMM 枚举 + `midiOutOpen`），
  而 OmniMIDI v2 的 WinMM 侧存在**空函数指针调用缺陷**（上游，进程内无法兜底）
  ——启动路径被触碰即可能在原生层 AV 杀进程。诊断结论：omv2 全责。
- **修复** —
  - XSynth/LGS 启动阶段**零 WinMM/midir 触碰**：跳过 System 快速启动/占位输出；
    `start_*_async_init` 改为返回 `bool`，仅提前 bail（无音色库/文件缺失）时才立即回退 System。
  - 播放输出延迟接线：`create_additional_output` 在软件后端初始化在途时返回 `None`
    （覆盖加载文档与自愈重试窗口）；`setup_playback_output` 同步延迟；就绪/失败后
    由既有重连路径创建（`reconnect_playback_output`）。
  - 启动输入枚举触发移除：`set_midi_api` 只注入 API（不再触发设备扫描）；枚举改由
    用户主动路径触发（录制开始时自愈）。
  - 失败兜底显式化：`finish_*_init`（Failed/Disconnected）才初始化 System 回退
    （`ensure_system_fallback_initialized`）；`reinit_if_needed` 切换到软件后端时
    同样不再先建占位。
  - LGS 初始化分段打点（GPU 上下文 / 音色库 / 设备解析 / 流启动耗时），便于定位
    原生层卡点（`api/lgs/init.rs`，自 `lgs.rs` 拆出）。
- **边界（D5，留作测试依据）** — ① System 后端 + 坏 omv2 仍会崩（omv2 环境问题；
  产品决策：Lumino 不再受理 OmniMIDI v2 任何接入需求，最多管 KDMAPI）；
  ② 软件失败 → System 回退路径仍触碰 WinMM；③ 录制/设置页属用户主动路径仍触碰
  midir；④ 初始化窗口期无占位试听（已接受）。
- **验证** — 本机 LGS 实测（构建含诊断打点）：启动日志零 WinMM 调用（无占位枚举/
  打开、无设备扫描）；LGS 后台初始化全程返回且各段耗时可见（GPU 330ms / 音色库
  1360ms / 设备解析 0ms / 流启动 88ms）；就绪后播放输出自动接线（策略 1）。
  单测：LGS/XSynth bail 返回值与提示、软件后端在途 `create_additional_output`
  守卫、`set_midi_api` 不触发枚举、显式扫描路径可用；受影响 crate 全绿
  （lumino-core i18n 13、lumino-midi-io 90、lumino-ui 470、lumino-rs 131），
  fmt/clippy 干净。

### UI-020 · LUMINO_LANG 环境变量泄漏修复（#150）

- **症状** — 排障用 `LUMINO_LANG` 启动（本应只影响本次启动），若顺手改了任意设置触发
  落盘，env 指定的语言被物化为「固定语言」偏好：老配置 × env × 任意保存三条件命中后，
  下次不带 env 正常启动也会被固定在该语言上。
- **修复** — `ConfigWrapper` 记录本次语言来源（env 是否实际生效）；持久化守卫：
  env 来源且用户本次未显式改语言时，`language` 与 `language_preference` **均不落盘**
  （`language` 一并守卫——老配置的解析会回退到该字段，单独放行仍会把 env 值带到下次
  启动）；用户显式改语言或非 env 来源时照常写入意图。解析优先级保持不变
  （env > 固定 > 老配置 > 系统检测）。
- **验证** — 新增单测：来源标记契约（env 生效/无效/无 env，含 ConfigWrapper 端到端）、
  落盘守卫真值表（4 组合）。

### REND-018 · 惰性创建的播放管理器未同步指示线位置（#151）

- **症状** — 加载工程后（管理器尚未创建）拖动演奏指示线再按播放，仍从工程开头起播
  （拖动产生的 seek 因 `manager.is_none()` 被静默丢弃，管理器内部位置为 0）。
- **修复** — `init_playback_manager` 末尾（`play()` 之前）注入
  `manager.seek(editor.playback_position)`（>EPSILON 时）：一次性覆盖所有「重建
  manager 后内部位置归零」路径；stop 后位置归零 → 仍从 0 起播。
- **验证** — 新增单测（并做过**反向验证**）：无 manager → 拖到 240 tick → Play →
  引擎首个非零位置 ≥239；禁用修复时该用例转红。

### AUD-005 · System (WinMM) 后端惰性初始化 + 设备扫描后台化（#149）

- **动机** — 无论选择哪个后端，System（WinMM）后端都会在 UI 线程同步创建 midir
  客户端（`System::new` 一次建 input+output），拖慢启动；播表扫描同样在 UI 线程
  执行；驱动异常时该同步调用无超时，可能拖死启动（与白屏反馈同源）。
- **修复** —
  - **midi-io**：`System::new()` 改为零 midir 的惰性句柄（首次 `outputs()`/`inputs()`
    访问时才创建，创建发生在调用线程）；连接类操作不变（每次自建独立实例）。
  - **UI 播表扫描**：改为**后台线程 + 信道轮询回写**；非 System 后端由函数内守卫
    直接拦截（调用侧删除重复判断）；扫描在途忽略重复请求。顺带修复一处死代码：
    `SettingsDialogAction::OpenDialog` 全仓无人发射，"打开设置页自动扫描"实际从未生效，
    现由 `set_settings_dialog_open(true)` 补齐真实触发（打开即后台扫描，System 后端）。
  - **UI 输入设备枚举**：移出启动路径（`set_midi_api` 不再同步 `inputs()`，改后台
    扫描 + 回写设备列表并自动选中首个；未注入 API 时置空），消除启动时以
    XSynth/LGS 后端仍创建 midir 输入客户端的路径。
- **验收口径** — 非 System 后端下，启动/开设置页不再包含"可后台化"的 midir 调用；
  XSynth/LGS 的 System 占位输出（快启动枚举/打开 + 播放输出连接）保持既有行为，
  其后台化/超时由后续卡处理。
- **验证** — 新增 5 项单测（守卫跳过非 System / System 异步在途并可收尾 / 播表写回与
  失效回落 / 输入设备写回与自动选中 / 无 API 置空）；全量回归 63 测试目标 0 failed；
  fmt/clippy 干净。

### KDMAPI 后端 · 兼容 OmniMIDI v2（AUD-001 #141）

- **症状** — OmniMIDI v2（15.0+）Windows 构建不再导出 `ReturnKDMAPIVer`，旧实现一次性
  `lib.get` 5 个符号、任一缺失即初始化失败——v2 用户完全无法连接，且错误信息无法区分失败阶段。
- **修复** — `ReturnKDMAPIVer` 改为**可选符号**：缺失时版本降级为 `unknown (OmniMIDI v2+)`
  且不阻断连接；其余 4 个必需符号逐符号解析，缺失时给出缺失名单与「可能为 OmniMIDI v2 /
  非 KDMAPI 构建」的可操作提示；`InitializeKDMAPIStream` 返回 0 的提示补充 v2
  settings.json（renderer/音色库）排查引导；`Symbols` 文档补齐 v1/v2 差异与来源链接。
- **可见化** — KDMAPI 启动失败回退 System 时，状态栏显示具体原因（与 #127 的 LGS 回退
  提示同模式），不再静默降级。
- **验证** — 符号解析层抽象为 `KdmapiLib`（fake 桩单测覆盖降级路径，不依赖真实 DLL）：
  新增 4 项单测（缺版本符号降级 / 缺必需符号名单 / 全齐 / 版本串契约）；全量回归 0 failed；
  fmt/clippy 干净。

### REND-017 · GPU 渲染缓冲状态指示器（#140，状态栏 + 设置可关）

- **动机** — REND-016 #139 之后，GPU (LGS) 渲染/缓冲状态对用户仍不可知：过载只有「突然
  无声/卡顿」，编辑时感觉迟钝却无法判断是否 GPU 渲染落后；排障与设置决策都缺可视反馈。
- **数据链路（无锁、零音频线程负担）** — `PlaybackStatsReader`（既有 relaxed 原子）
  → gpu-synth 新增 `GpuBufferStats::snapshot`（含 `samples_per_second` 交错样本换算基准，
  重采样时按设备率）→ midi-io `SynthControl::gpu_buffer_stats()` 扩展能力（默认 None，
  仅 LGS 覆写）→ Runner 每 250ms（4Hz）轮询经 `Host::set_gpu_buffer_status` 注入状态栏。
- **状态栏指示器** — GPU 组：`Q 248ms→`（队列深度，毫秒整数 + 趋势箭头）、`LOAD 63%`
  （颜色分级：<60% 常规 / 60–90% 黄 / ≥90% 红）、`V`（活跃声部）、`UR`（欠载计数，>0 红）；
  异常时附 `GOV`（REND-016 治理级别）与 `DROP`（丢弃计数）。非 LGS / 后端未就绪自动隐藏；
  未播放也显示（渲染线程常开、数据真实，直接服务「编辑时排障」）。
- **播放统计口径** — `UR`/`DROP` 展示「播放会话」累计：从停止态按下播放时清零重新计数
  （暂停/恢复不打断累积，停止后数字保留），下次起播再清。配套 3 项单测
  （清零范围 / 控制通道 / 起播时序）。
- **设置（音频→LGS 区）** — 三项，均持久化且**不触发后端重建**（纯显示项）：
  `gpu_buffer_status_enabled`（总开关，默认开）、`gpu_governor_metrics_enabled`（GOV/DROP
  副开关，默认开）、`gpu_load_display`（LOAD 口径：3 秒平均〔默认〕/ 瞬时）。
- **关键设计决策** — ① 轮询快照而非句柄传递：端口布局重建会整体替换 `AudioPlayback`
  （旧 reader 冻结失效），轮询经最新 api 现场读天然免疫；② `STATS_RING` 128→1024
  （8KB）支撑 3 秒短窗（默认 512@64k → 375 块；小块配置夹紧到环容量）；③ 队列以毫秒整数
  显示（量程 0–600ms，容量按时间封顶 32ms–600ms），颜色与趋势承担危险度表达。
- **附带发现（不在本卡修复）** — `flush_epoch`「过载重同步清队列」为死代码（无任何递增点，
  `7bb878b9` 改「宁糊不切」后遗留），其清队列路径永不触发，不影响本卡指标准确性。
- **验证** — 新增 11 项单测（gpu-synth 窗口/换算 2、midi-io 窗口换算 1、core 配置缺字段/往返 2、
  状态栏分级/趋势/变更检测 4、设置面板镜像/事件 2）；`cargo test --workspace` 全绿（0 failed，
  含 doctest）；`cargo fmt --check` / `cargo clippy --workspace --all-targets` 无新增警告。
  记录见 `docs/2026-10-11-REND-017-GPU缓冲状态指示器-实施记录.md`。
- **遗留** — 真机复验（LGS 播放实时性 / #139 过载场景肉眼可见 / 开关持久化）待现场执行。

### 音符画 · 描边预览改为「画会生成的音符块」（BUG 修复：矩形/曲线的宽度显示 X2）

- **症状（用户报）** — 粗细度是"中间铺开"的：调大后画布上**预览**从线条两侧一起变宽，
  而 √ 生成的音符带只向**外侧**铺——矩形工具的宽度改变后预览显示是实际产物的 **2 倍**
  （最典型：粗细 2 时竖直边预览 2 层、产物 1 层）。
- **根因（RCA）** — 两条腿各画一套几何：产物走 `stroke_band`（沿线段法线**单侧向外**
  铺 `N-1` 层），预览走 `frame.stroke(.., Stroke::width = 全局粗细 × 单 key 高)`
  ——`Stroke` 是**沿路径居中**铺开。于是竖直方向上预览 = `N` 层、产物 = `N-1` 层，
  比值 `N/(N-1)`（粗细 2 时正好 2 倍）。这与本仓既有的「所见即生成」纪律冲突：
  填充预览（`line_tool_box` 的 block 画法）与画刷预览（`brush_pending_notes_by_stroke`）
  早就都是"画会生成的音符块"，只有描边预览还在用线宽参数表达宽度。
- **修复（渲染）** — 描边预览直接画 `√` 会生成的**音符块**：新增
  `Editor::shape_outline_blocks(spec, filled)` / `Editor::path_outline_blocks(path)`，
  与写入腿共用同一个 `stroke_band::outline_blocks`（新增，唯一入口：`outline_notes`
  → key 过滤 → `to_note_triple` 长度补齐 → `(起 tick, 止 tick, key)` 块）。
  块在本仓的格子口径是 `[key-0.5, key+0.5)`（与音符渲染、填充预览一致），抽成
  `grid::utils::fill_note_blocks`（填充预览的 block 画法一并改用它，消灭第二份格子口径）。
  于是"预览的宽度"= "产物的宽度"是**结构保证**，不再依赖两处参数碰巧一致。
- **配套（引导线）** — 矢量轮廓降级为**固定宽度**的引导线
  （`OUTLINE_GUIDE_WIDTH = 1.5px`，与粗细**无关**）：它只回答"我画的线在哪"，与锚点/
  控制柄同类；跟着粗细变宽正是本 BUG 的来源。填充图形不再画随粗细变宽的边框——
  填充腿不生成描边、也不吃粗细，那条边框等于显示一个不会存在的产物。
- **口径收窄** — `Editor::draw_line_width_px`（层数 × 单 key 高）语义收窄到**画刷**
  （笔画命中带 `brush_total_width_px`）；曲线 / 形状的描边预览不再引用它。
- **反向验证（证明新测试有效）** — 把预览的粗细参数固定成 1：两条不变式用例全红；
  把预览的长度下限参数改成 `1.0`：补齐用例红并打印出 `(800,801)` 未补齐的块
  （产物是补齐到 60 tick 的 `(800,860)`）。
- **验证** — 全部 lib 测试 **1401 全绿**（+4：形状描边预览 == 写入 / 补齐长度同源 /
  填充图形无描边预览 / 曲线描边预览 == 写入）。新增用例都做过**反向验证**（见上一条）。
- **画刷不动** — 按用户口径：画刷工具（逐层铺 key + 命中带 + 方块预览）保留原样。
- **残留** — 预览与产物现在逐块一致；仅剩"预览块用实色、注释/锚点用引导线"这类视觉
  层次差异（不表达宽度），需视觉签核。

### 音符画 · 加粗带的**拐角补齐**（BUG 修复：矩形右上角出现缺口、音符没盖到角上）

- **症状（用户报）** — 矩形工具描边确认后，**右上角出现异常缺口**：音符没有覆盖到角上。
  粗细越大缺口越大（缺口尺寸 = 加宽量 × 加宽量）。
- **复现（修复前实测）** — 矩形 `0..800 × 60..64`、粗细 3、默认 zoom（1 tick = 0.1px、
  1 key = 20px）：`key 65` 行只有 `[(0,800)]`，即外角方格 `tick 800..1200 × key 64..66`
  （实机 **40×40px**）整块没有音符。
- **根因（RCA）** — 加粗带是「每段折线一条单侧矩形 → 按行求并」。上边的矩形止于角点的
  tick（800）、右竖边的矩形止于角点的 key（64），**谁都不越界到 `tick > 800 ∧ key > 64`**，
  于是两段的并集在拐角**外侧**缺一块方格。为什么只有右上角缺：法线定号是全局的
  「向上优先、其次向右」，只有"向上 + 向右"同时外扩的那个角（右上）两条边会双双逃出，
  其余三个角至少有一条边的带盖住。为什么预览没有这个缺口：预览走 `frame.stroke`
  （`Stroke` 默认 **Miter** 拼接，拐角实心），生成侧漏了预览有的那块。
- **修复（几何）** — `band_notes` 增加**拐角补齐**：以每个顶点为起点，用相邻两段的
  **偏移向量张成的平行四边形**填外角（垂直拐角时正好是缺掉的那个正方格；斜拐角时是其
  外切方格——比真 miter 略大、比留缺口安全）。顶点范围：闭合环（首点 = 尾点，判定与
  `paths::path_notes` 同一条）每个顶点都补，顶点 0 的"前一段"取最后一段；开放折线只补
  内部顶点（两端是 Butt 端帽，与预览默认端帽一致）。共线（同向）拐角的平行四边形零面积
  → **必须跳过**（像素度量下的叉积判据）：不跳时退化四边形会被 even-odd 配成零宽区间、
  再被"至少 1 tick"兜底成**直线上的一列碎音符**（实测：去掉守卫后 key 63/64 各多一条
  `(800,801)`）。
- **同源一次性覆盖** — 曲线折线、形状描边（矩形/三角/圆）共用同一个入口
  `stroke_band::outline_notes`，所以拐角补齐对三条链路同时生效。圆不单独写用例：它就是
  闭合折线（64 段），拐角走的是与"矩形首顶点回绕"同一条代码路径（该用例已单独钉死）。
- **顺带修掉（同一函数，探针发现）** — 偏左法线（如向右上的斜段）会让带在 tick 0
  左侧外溢到负 tick；`keep_longest` 按 `(start, key)` 分组，`(-21, k)` 与中心线的 `(0, k)`
  算成两组，等调用方 `to_note_triple` 把起点钳到 0 之后就留下**两条同 start 同 key 的
  重叠音符**（渲染叠层、历史里两条）。改为在 `band_notes` 内就钳起点，并丢弃完全落在
  `tick < 0` 的区间（否则钳完会凭空多一条 tick 0 的 1 tick 假音符）。
- **已核实、非缺陷** — 曲线工具下加粗带用**展平折线**、中心线用**首尾拉伸后**的折线
  （`path_notes` 的 `stretch_ends`）：两者行归属一致（拉伸落在 `EDGE = 0.5 - 1e-6` 内、
  不跨行），且带的行区间**包住**中心线（45° 斜线探针实测：`key 63` 带 `[439,3360)` ⊇
  中心线 `[2304,3072)`）→ 无缺口；仅"脊线"在带内被重新分配，肉眼不可见。形状不受影响：
  闭合环从不走拉伸。
- **验证** — 全部 lib 测试 **1397 全绿**（+6：拐角 4 条 + 负起点钳制 1 条 + 形状端到端 1 条）。
  新增/变更用例都做过**反向验证**：临时关掉拐角补齐循环，4 条拐角用例全部失败
  （`65: [(0,800)]` vs 期望 `[(0,1200)]`）；临时关掉共线守卫，`test_collinear_vertex_adds_no_note`
  失败并露出碎音符；临时去掉起点钳制，`test_band_clamps_negative_start_before_dedupe`
  失败并列出 4 条负起点音符。
- **边界** — 粗细 1 仍**逐位不变**（`band_notes` 直接返回空，有断言）；斜拐角的补齐量取
  "外切方格"而非真 miter 交点（近 180° 反向折角无交点、真 miter 还需 miter limit）——
  少的那点尖角远小于"留缺口"，且预览端帽/拐角本就是实心。

### 音符画 · 全局粗细的加粗改为「沿线段法线」（BUG 修复：竖直边/竖直线永远变不宽）

- **症状（用户报）** — 调大粗细度后，矩形**竖直边**（以及曲线工具画的竖直线）仍然是
  "很细一条"，宽度不跟着粗细度走；水平边则正常变厚。
- **复现（修复前实测）** — 默认 zoom（1 tick = 0.1px、1 key = 20px）下：
  - 竖直线（曲线工具，key 60→64）粗细 3：每条音符宽度 **1 tick**（应为 399+ tick）；
  - 矩形 0..800 × 60..64 描边粗细 3：key 63 行上左右竖边宽度 **1 tick / 1 tick**
    （应为 400 / 400 tick）。
- **根因（RCA）** — 上一版的加粗实现是「把 `path_notes` 出的中心线音符按 `key + level`
  **向上平移** N 层」（`paths::expand_layers`）。**平移不是加粗**：它只在"平移方向恰好
  垂直于线段"时成立——水平段刚好如此，竖直段被平移等于把线**拉长**（横向宽度原地不动），
  斜段则被按"水平段"硬套、方向也错。用户看到的"竖边很细一条"就是这个几何错误。
- **修复（几何）** — 新增 `interaction/line_tool/stroke_band.rs`：加粗沿**线段法线**做。
  每段折线构造一条**单侧矩形**（段两端各沿法线偏移 `(N-1)·zoom_y` 像素；法线定号为
  「向上优先、其次向右」，与画刷笔画"从落笔 key 向上铺层"同偏向、且与线段走向无关），
  再用填充引擎同一套逐音高行区间解析（`fill::spans::row_spans`，凸四边形下 even-odd
  即内部）求出该段在每一行的 tick 区间，全部线段**按行求并**
  （`spans::merge_spans` 提升为 `pub(crate)` 复用，不再写第二份区间合并），
  最后与中心线音符走既有 `keep_longest` 合并（中心线的短音符被更长的带吸收，
  不会在带里留下细线夹层）。于是：水平段纵向加厚 N 行（与上一版口径一致）、
  **竖直段横向加宽 `(N-1)·zoom_y/zoom_x` 个 tick**、斜段沿法线加厚。
- **修复（同源）** — 曲线轮廓与形状描边走**同一个入口** `stroke_band::outline_notes`
  （`line_tool/confirm.rs` 的写入腿 + 逐路径归属腿、`shape_tool/confirm.rs::outline_notes`），
  旧的 `paths::expand_layers` **删除**——两条链路各自长一套加粗口径正是这类 BUG 的温床。
- **与卷帘方向无关** — `zoom_x` / `zoom_y` 的含义是"每 tick / 每 key 多少像素"，纵向卷帘
  只是让这两个方向占据不同的屏幕轴（`coords.rs` 的转置），像素度量不变；因此加粗模块
  **没有任何卷帘分支**，两个方向天然一致。
- **口径与边界** — ① 粗细 1 时加宽量为 0，`band_notes` 直接返回空、出口与加粗功能引入前
  **逐位一致**（有断言）；② 中心线那一层仍由 `path_notes` 出，带只补 `N-1` 层；
  ③ 越界行裁到 `key_hi`；④ 极端缩放（`zoom_x` 远大于 `zoom_y`）下横向加宽不足 1 tick 时
  取整为 1 tick——这是 tick 量化的下限，再由最小长度下限兜底。
- **同族缺陷（已确认，本轮未擅自动）** — **画刷笔画**的粗细仍是"每格按 key 向上铺 N 层"
  （`cov::expand_layers`，逐层对应一条音轨）：竖直笔画同样只被"拉长"而不加宽，
  且它的**命中带**是均匀宽度（`brush_total_width_px = N·zoom_y`）——于是竖直笔画出现
  "抓得住 40px、画出来 1 格宽"的内在不一致。它**不是**本次这种"显示与产物不一致"的 BUG
  （画布上的画刷预览与生成**同用**这一套，彼此一致），而是**模型层面**的同族缺陷：
  修它要把"层"从"key 层"改成"距中心线的层距"，牵动逐层音轨分配语义、绘制行为对话框、
  bench 与 15 个测试文件——属于需要单独签核的口径变更，本轮只落报告与证据。
- **残留（显示侧，非 BUG）** — ~~画布预览是**居中**描边、生成带是**单侧**铺开，
  对齐差半个带~~ → **已按用户口径"统一为向外拓展宽度"修复**：预览直接画会生成的音符块
  （见上一节「描边预览改为画会生成的音符块」），两者逐块一致；画刷保留原样。
- **验证** — `cargo test -p lumino-ui-editor -p lumino-ui -p lumino-ui-core -p lumino-core
  -p lumino-extras -p lumino-message --lib` **1391 全绿**（ui-editor 756 / ui 456 / ui-core 73 /
  core 30 / extras 29 / message 47）。新增 8 项：
  - **端到端复现（两条链路各一条，修复前必失败）**：
    `test_confirm_vertical_line_widens_by_global_thickness`（曲线竖直线：粗细 3 下**每条**
    音符宽度必须 ≥ 399 tick；修复前实测 `[1,1,1,1,1,1,1]`）、
    `test_rectangle_vertical_edges_widen_by_global_thickness`（矩形竖边：取 key 63 作量尺
    ——下水平边铺到 62、上水平边从 64 起，只有左右竖边覆盖该行，宽度可被完整量出；
    粗细 1 = `[(0,1),(800,801)]`、粗细 3 = `[(0,400),(800,1200)]`）；
  - **加粗几何单测（新 `stroke_band/tests.rs`，6 项）**：粗细 1 不加粗且出口逐位等于中心线生成、
    退化输入（空/单点/零缩放/零长度段）不 panic 不产垃圾、水平段只向上铺 N 行且 tick 区间不变、
    **竖直段横向加宽到 `(N-1)·zoom_y/zoom_x` 且与线段走向无关**、斜段沿法线（中段行弦长
    ≥ 1.2 × 带宽、带上界超出线段 key 上界）、`outline_notes` 把中心线短线吸收进带、
    越界行裁到 `key_hi`。

### 音符画悬浮工具条 · 「粗细」控件落地（向上弹出面板 + 滑杆 1-100）+ 全局粗细三处同源

- **控件** — 右端「粗细」按钮改为**开合**语义（`ToggleThicknessPanel` / `CloseThicknessPanel`），
  弹出**窗口内锚定面板**（`CurveToolGroup` 同一套悬浮层，**向上弹出**；顶部空间不足才下翻），
  面板内是**一根滑杆**：量程 `1..=100`、**步进（精度阈值）1**，就地显示"当前值 + 单位 + 量程"，
  底下一行说明文案。量程两端**直接取 `BrushConfig::MIN/MAX_THICKNESS`**——滑杆右端与
  `set_thickness` 的 clamp **同源**，改一处即同步；把上限从 20 放宽到 100 就是这一步，
  否则滑到头却写不进去（静默截断）。
- **「粗细」= 全局绘制粗细（纵向 key 层数）** — 需求口径「粗细全局」落在**一份值**上：
  物理载体 `BrushConfig::thickness`，唯一权威 `Toolbar::brush.thickness`（面板滑杆写、
  画刷下拉**只读**展示、绘制行为对话框增删层也改它），经既有通道
  `handler.rs` 的 `editor.brush = toolbar.brush.clone()` 全量同步进编辑器。
  `BrushConfig` 的类型注释已从"画刷私有"改写为**全局绘制粗细**（字段名保留：换成新类型/新字段
  会连带改对话框、历史载荷与 bench，收益不抵风险）。
- **三处同源消费（本仓「所见即所得」纪律的落法）** — 一根滑杆同时管**产物**与**显示**：
  1. **画刷笔画**：每格按 key 向上铺 `thickness` 层（既有 `cov::expand_layers`，逐层音轨分配不变）；
  2. **曲线 / 形状的描边**：生成前按全局粗细加粗（`line_tool/confirm.rs` 的写入腿 +
     逐路径归属腿、`shape_tool/confirm.rs::outline_notes`）。加粗口径在同期的下一条
     CHANGELOG 里从"按 key 向上平移 N 层"**修正为沿线段法线**（竖直段永远变不宽即其 BUG），
     这里不再复述中间态；
  3. **画布线宽**：新增 `Editor::draw_line_width_px()` = 层数 × `zoom_y`（下限 1px）。
     曲线段（原硬编码 `LINE_WIDTH = 4.0`）、待确认形状轮廓（原字面量 `2.5`）、
     画刷笔画总宽（原 `brush_total_width_px` 自算）**全部收敛到这一个函数**，
     于是"画出来多粗"与"√ 后生成多高"严格同源。
- **有意不动的地方** — ① 颜料桶**填充**腿：面积不是笔画，铺层会让覆盖越过图形边界
  （填充腿连最小长度下限都不套，同一理由）；② 文字工具：字形栅格化的 key 跨度由文本框决定；
  ③ `drawn_shape_box` 的常显轮廓 / 选中高亮 / 框选：那是"可选中范围"的 UI 提示，不是画出来的线；
  ④ `fill_division` 切分档位（只作用于填充）。
- **删掉的重复控制** — 画刷下拉里的「粗细度 −/+」步进按钮**删除**（同一份值摆两个控件、
  且旧按钮的上限硬编码 20——正是"看起来能调、其实两处打架"的那类坑），改为**只读**显示当前值、
  保留「绘制行为」入口；随之删除 `Event::BrushThicknessChanged`、死分支
  `BrushSettingsAction::ThicknessChanged`（有 handler 无 emitter）及其构造器。
  绘制行为对话框的层标签由「粗细度 N」改为「第 N 层」（N 是层序号，粗细值本身归滑杆管）。
- **已知边界** —
  - **行为变化（需视觉签核）**：粗细 > 1 时曲线 / 形状描边的**生成音符数量与纵向覆盖都会变**；
    层副本会在同一 `(tick, key)` 上相遇并被既有"同 tick 同 key 只留最长"规则收缩，
    故条数 < N × 单层条数（矩形粗细 3 的实测：单层 10 条 → 加厚带覆盖 key 60..=66）。
    默认值 1 **逐位不变**（有断言钉死）。
  - **量程上限 100 是重负载**：画刷预览块数 = 可见格 × 粗细，bench 一直把"粗细度 20"当病态档
    （`ui_brush_stroke_bench`）；100 层意味着同一笔最多 5 倍于该病态档。本轮按需求把滑杆开到 100，
    **未加额外钳制**——真机若卡，先看"笔画点数 + 粗细 + zoom"。
  - **跨会话不持久化**：粗细沿用画刷配置的既有行为（不入 `UiConfig`、切工程不重置、重启回落 1）。
    要不要落盘是下一轮的口径（可照 `automation_line_thickness` 那条"字段 → 设置页 → 实时生效 →
    关窗 diff → 落盘"的链）。
- **验证** — `cargo test -p lumino-ui -p lumino-ui-editor -p lumino-core -p lumino-ui-core
  -p lumino-extras -p lumino-message --lib` **1386 全绿**（ui 456 / ui-editor 751 / core 30 /
  ui-core 73 / extras 29 / message 47）；clippy 0 warning；fmt 干净。新增 / 改写 14 项：
  - **控件**：`test_thickness_panel_toggles_without_touching_tool_state`（开合 + 不动工具 /
    填充 / 悬浮条位置）、`test_thickness_panel_stays_open_while_dragging_slider`
    （**拖滑杆不得把自己关掉**——guard 里显式放行 `DrawThicknessChanged`，这是最容易被写成
    "一拖就关"的一处）、`test_draw_thickness_changed_clamps_to_slider_range`（0→1、200→100，
    且 `MAX_THICKNESS == 100`）、`test_slider_range_matches_clamp_range`（**滑杆右端必须写得进去**，
    钉死"滑到头却没变化"）、`test_thickness_panel_exclusive_and_closed_by_external_actions`
    （与其余窗口内面板互斥 + 外部动作收起）、关条即收面板（`test_close_tool_panel_closes_settings_dropdown`
    扩写：悬浮条不在就没有锚点）、以及面板渲染冒烟（两种语言 × 边界值）+ **宽度护栏**
    `test_thickness_dropdown_fits_menu_width`（两种语言 × 粗细 1 / 100：内容超宽会溢出面板背景，
    只测中间值会漏掉三位数那一档）；
  - **归属**：`test_tool_settings_menu_owner` 增加 `SettingsPanelOwner::Thickness`——
    面板锚定在右端按钮（不是工具条目），否则会以胶囊中心为准居中、看起来贴在悬浮条左端；
  - **契约**：`test_thickness_button_contract` 改为只发 `ToggleThicknessPanel`（不得退化为
    工具条目、不得与齿轮共用消息）+ 面板内空白发 `CloseThicknessPanel`；
  - **端到端**（`root/root_tests/draw_thickness_entry.rs`，4 项）：窗口内开合且**不冒发任何
    窗口事件**、**滑杆值一路同步到编辑器**（`editor.brush.thickness` / `draw_thickness()` /
    `draw_line_width_px()` 三者同源）、不扰动工具与填充态、与齿轮同口径收起分音符填充面板且不丢档位；
  - **几何**：曲线确认 2 项（3 层 → key 60/61/62 同 tick 区间；默认 1 层逐位不变）＋形状描边 1 项
    （矩形粗细 3 → 带状覆盖 key 60..=66 每行无空洞，条数落在 [单层, 单层 × 3]——钉
    **覆盖不变量**而不是某个依赖路径行走顺序的定值条数）＋当时那套"按 key 向上铺层"的
    4 项单测（已随下一条 CHANGELOG 的口径修正一并删除：它的前提本身就是 BUG）。

### 音符画悬浮工具条 · 右端「设置区」新增「粗细」按钮（入口 + 图标）

- **新增** — 音符画悬浮工具条**右端设置区**新增一枚**常显的「粗细」按钮**，落在齿轮
  **左侧**（设置区自左向右：粗细 → 齿轮；两者与工具图标区之间仍以分隔线隔开），
  为与其它条目同规格的 34px 图标按钮，带悬浮 tooltip（新文案键 `tool_thickness`：
  「粗细设置」/「Stroke Width」）。
- **图标** — 复用仓内既有 `resources/icons/toolbar/stroke-settings.svg`（上下两条粗细不同的
  横线）。该文件与 `Icon::StrokeSettings` 自旧「绘制工具下拉」起就一直挂在图标表里，
  却**从未被任何视图渲染过**——全量图标回归只证明"能解析、能光栅化"，不证明"真的出现过"。
  本轮把它激活，枚举名收敛为 `Icon::Thickness`（**文件名不动**：SVG 术语 `stroke-width`
  就是线宽/粗细）。交付前用 crate 自身的 `usvg + resvg` 管线把它光栅化到 **22px**
  （悬浮条内实际尺寸）做目视签核：粗线占 4 行、细线占 2 行（≈3.7px vs ≈1.4px，对比
  2.6 倍，一眼可分），内容垂直占据第 5..16 行、上下留白 5 / 6 行（居中偏差 ≤ 0.5px），
  横线两端圆帽不越界。
- **入口的落地** — 该按钮最终是**开合窗口内面板**（全局粗细滑杆）的入口：面板、量程、以及
  "一根滑杆同时管显示与产物"的口径见**上一条**（同一 Unreleased 周期内接完，此处不再叙述中间态）。
- **为什么是专属事件，而不是复用 `ToolPanelItem`** — 旧枚举里有个 `StrokeSettings` 变体，
  自加入起就是**空分支 + 不可达**（悬浮条只渲染 6 个工具，从未渲染它）：它的存在方式正是
  "把设置入口塞进工具条目枚举"的代价——`apply_tool_panel_item` 按**选择语义**改
  `current_tool` / toggle `fill_enabled`，设置入口得处处写例外。本轮**删除该变体**
  （`ToolPanelItem` 收敛回纯"工具条目"），`Toolbar::update` 与 `ToolbarHandler` 里对应的
  两处空分支一并清掉；右端两枚入口（粗细 / 齿轮）各带专属事件，可独立接落地。
- **文案收敛（顺带清死键）** — `tool_stroke`（「描边设置」）→ `tool_thickness`（「粗细设置」）。
  「描边」在本仓另有确定含义（曲线 / 形状轮廓的**音符生成口径**，见下一条 CHANGELOG），
  与"线宽粗细"混用会让同一条悬浮条像两个系统；旧键 `tool_stroke` **无任何读取点**，
  随之一并删除（不留两个近义键）。
- **验证（入口阶段口径）** — 当时 `cargo test -p lumino-ui --lib` **449 全绿**、
  `-p lumino-ui-core --lib` 73、`-p lumino-extras --lib` 29；clippy 0 warning、fmt 干净。
  该阶段新增的 **几何**断言仍在（真实 iced 布局，未走"无适配器即跳过"分支）：
  `test_thickness_entry_sits_left_of_gear_in_settings_group`——粗细在**全部工具图标右侧**、
  在**齿轮左侧**、与齿轮**恰好隔一个按钮间距**（4px，即"右端两枚设置入口"的形态承诺）、
  标准 34px 规格；顺带把两条几何断言的布局前置抽成公共 `layout_row_slots`
  （两处各写一遍 `Tree::new` + `layout` 迟早会在视口尺寸 / `Limits` 上漂移）。
  该阶段的契约 / 端到端用例已随控件落地**改写**为上一条里的最终形态。

### 音符画悬浮工具条 · 新增「设置按钮」（悬浮条右端齿轮 → 独立对话框窗口）

- **新增** — 音符画悬浮工具条**右端**新增一枚**常显的设置按钮**（齿轮图标，
  `resources/icons/regular/settings-general.svg`），以分隔线与 6 个绘制工具图标区隔；
  点击打开**独立 OS 窗口**「音符画设置」（新 `DialogType::DrawSettings`，460×420，
  与「画刷绘制行为」同为 `Host::new_dialog` 家族）。
- **为什么单开一个入口** — 现有三块工具设置（画刷下拉 / 形状下拉 / 分音符填充）都藏在
  「条目已启用后**再次点击**」这个两段式手势后面（见下一条 CHANGELOG 的交互改造）：
  想调参数得先激活对应工具、再点第二次。齿轮是**聚合入口**，且**不参与工具选择语义**
  ——点它不会顺手把当前绘制工具切掉（用户只是想调参数，不是想换工具）。
- **为什么是独立窗口而不是主窗口内的锚定小面板** — 悬浮条小面板宽度固定 248，
  容纳不下成组的设置项（下拉 / 输入 / 列表）：要么溢出面板背景，要么被迫做成滚动小窗，
  反而比独立窗口更难用；独立窗口有系统标题栏、可拖动、可独立缩放，且与既有设置类入口
  （画刷「绘制行为」）一致。**顺带删掉一整类复杂度**：主窗口内不再有这个面板，
  于是不需要锚点悬浮层、不需要开合状态位、不需要与画刷 / 形状 / 分音符填充三块窗口内
  面板做互斥（`Toolbar::draw_settings_open`、`ToggleDrawSettings`/`CloseDrawSettings`
  两个事件、上一版为此加的画布 Ctrl+单击互斥补丁与视图层互斥 `debug_assert` 一并移除）。
- **本轮范围 = UI 入口** — 对话框骨架（标题 + 占位说明 + 底部「关闭」）+
  「设置项开发中」占位文案（中英双语：`tool_panel_settings` / `draw_settings_placeholder`）。
  **不做假控件**：摆一排点了没反应的控件只会再造一个"能点到、没反应"的静默失效。
  占位阶段**不摆「保存」**——没有可保存的改动，摆上去只会让人以为存了什么
  （与画刷对话框的 保存/取消 不同：那边有真实草稿）。
- **实现** — 完整复刻「画刷绘制行为」这条已验证过的独立窗口链路：
  `DrawSettingsAction`（新，`lumino-message`）→ `Message::DrawSettings` →
  `DialogHandler` → `window::Event::OpenDrawSettingsDialog` → Runner 开窗 →
  `DialogType::DrawSettings` → `Host::new_dialog` + `set_draw_settings_dialog_open` →
  `overlays.rs` 分发到 `view_draw_settings_dialog`。**开窗去重**：齿轮是"开合"语义，
  但独立窗口不能靠再点一次关闭（主窗可能不在前台），故 Runner 对同类型窗口去重
  （已存在则 `focus_window`），且判定用**新增的 `DialogManager::is_open_or_pending`**——
  对话框是**分帧三阶段**创建的（建窗 → GFX → UI），这期间窗口既不在 `dialogs` 里、
  也已移出 `pending_dialogs`，只看已就绪窗口会在连点两次时真的开出两个窗口。
- **验证** — `cargo test -p lumino-ui --lib` **412 全绿**；`lumino-dialog` 6 / `lumino-message` 47
  全绿。新增 / 改写：
  - **端到端**（重写 `root/root_tests/draw_settings_panel.rs`，走 `Root::update` 全链路，
    6 项）：齿轮发 `OpenDrawSettingsDialog` 窗口事件且悬浮条保持打开、
    **点齿轮不扰动当前工具与填充态**（齿轮退化成工具条目 = 点设置顺手切走工具，这条钉死它）、
    与其它工具栏动作**同口径**收起窗口内浮层（并断言收起不丢已输入的填充档位）、
    对话框 Root 按 `DrawSettings` 渲染不 panic、「关闭」→ `DialogResult::Cancel`、
    关闭动作不冒发"打开"事件（避免关窗把窗口又开回来）；
  - **几何 / 契约**：新增 `test_gear_entry_sits_at_right_end`——齿轮左缘 ≥ 全部工具图标右缘、
    按钮为标准 34px 规格（"右端设置按钮"这个需求点的几何钉死）；
    `test_settings_panel_is_centered_on_its_trigger_button` 收回到 **3 个**窗口内入口
    （画刷 / 形状 / 颜料桶；齿轮已不在主窗口落浮层，没有"居中于按钮"可言）；
    `test_gear_settings_button_contract` 改为断言齿轮**只发** `OpenDrawSettingsDialog`
    （不再有配套的"关闭/切换"事件）；
  - **状态机**：`test_open_draw_settings_dialog_leaves_tool_state_untouched`、
    `test_open_draw_settings_dialog_is_idempotent`（连点两次无状态变化 = 无开合语义）；
  - **去重**：`test_is_open_or_pending_sees_queued_dialog`（钉死"排队中也算已开"，
    并区分它与 `has_dialog_type` 的语义差异）；
  - **文案**：`test_draw_settings_dialog_texts_are_non_empty`（标题 / 占位说明 / 关闭按钮
    在两种语言下都不得为空——空标题会渲染成一个只有边框的空白对话框，既不报错也不崩溃，
    是 UI 上最难定位的一类静默失效）。
- **一处刻意的不一致，实测后改回统一** — 初版让齿轮"例外放行"分音符填充面板
  （理由：独立窗口与它不在同一窗口争位置）。写完测试立刻暴露矛盾：点**工具条目**会收起
  该面板、点**齿轮**却不会，同一动作下三块窗口内面板行为不一致。窗口内面板本就是
  "外部一动作即收起"的临时浮层，且输入值存于 `Root::state`、收起不丢数据，
  故撤掉例外、收敛到既有语义（对应 `test_gear_closes_in_window_panels_like_any_other_action`）。
- **已知共同边界（非本轮引入，现已不适用）** — 上一版「主窗口内锚定面板」曾有一条
  共同边界：面板不响应"点击画布空白即关闭"。改为独立窗口后该问题自然消失
  （窗口有自己的标题栏与系统关闭按钮，且不遮挡画布操作）。

### 音符画 · 文字工具补齐纵向卷帘支持（文字「正着读」）（BUG 修复）

- **症状** — 纵向卷帘下选中「文字工具」在画布上拉框有反应，但**松手后什么都没有**：
  看不到文本框、看不到 √/×/模式 三个按钮、也弹不出文字输入框；再点一下框还会被
  取消掉。表现为"能选中、点了没反应"的静默失效（音符画悬浮工具条 6 个在架工具中
  唯一不可用的一项）。
- **根因** — `grid/text_tool_box.rs::box_rect_screen`（文本框、√×/模式按钮、
  `TextInput` 覆盖层的**唯一**几何来源）在纵向直接 `return None`：
  `confirm_text_tool` 无入口（其唯一调用点是 `handle_text_tool_pressed` 的按钮命中分支，
  而按钮命中又依赖 `button_rects` → `box_rect_screen`），链路三处一起断。
- **修复（几何）** — `box_rect_screen` 增加纵向转置分支：key→X（覆盖整行 key，
  右缘 = key_hi 右边界）、tick→Y 且 **tick 越大越靠上**（与 `Editor::tick_to_y_vertical`
  同口径）。按钮、输入框、框内拖动随之全部自动可用（无需各自特判）。
- **修复（轴角色：文字必须正着读，关键）** — 新增 `interaction/text_tool/rasterize.rs::GlyphGrid`
  作为「字形列/行 → 逻辑轴」的**唯一权威**：横向「列→tick（+snap）、行→key（−1）」；
  纵向**「列→key（+1）、行→tick（−snap）」**。纵向视图的屏幕轴是 X = key、Y = tick，
  文字要正着读就必须让 advance 沿屏幕 X、字形"向下"沿屏幕 Y；若照搬横向挂法，字形铺到
  屏幕上会变成旋转 90° 且翻转的**镜像**（斜着/反着，读不了）。轴角色确定后预览位图
  **不需要任何转置**（位图 x 即屏幕 X、位图 y 即屏幕 Y），铺进框矩形即为正立可读。
  采样分辨率同步互换：横向「列 = 时间格、行 = 音高格」，纵向「列 = 音高格、行 = 时间格」。
- **修复（合并模式合并方向）** — 「key 范围合并」恒为**沿时间轴**合并：横向 = 同一 key
  行内连续列、纵向 = 同一 key 列内连续行，音符长度 = 该段的时间跨度。若照搬"按行扫描
  合并"，纵向的合并方向会落到音高轴上，于是"音高跨度"被当成"音符长度"写入文档
  （长度单位错乱）。
- **修复（交互口径）** — `handle_text_tool_pressed` / `handle_text_tool_moved` 里写入
  `EditState::Selecting` 的 stored Y：纵向改取 tick 的屏幕 Y（原先无条件用
  `view.key_to_y()` 的横向"key 行上/下边"语义，纵向无意义；虽被纵向渲染分支忽略，
  但属于同类隐患，一并收敛）。
- **验证** — 新增 8 项回归：`test_box_rect_screen_vertical_transposed`（独立算术钉死
  key→X / tick→Y，含"tick 越大越靠上"方向断言）、`test_button_rects_vertical_inside_content`
  （三按钮存在、横向排布、完整落在卷帘内容区内）、
  `test_vertical_preview_bitmap_axes_match_screen_axes`（**正着读判据**：列增→屏幕 X 增、
  行增→屏幕 Y 增，且列数 = key 跨度、行数 = 时间格数）、
  `test_vertical_glyph_grid_matches_preview_and_notes`（预览位图格矩形 ≡ 生成音符的屏幕
  矩形，逐格重合）、`test_vertical_merged_merges_along_time_axis`（纵向合并沿时间轴、
  长度 = 时间跨度、空隙断开）、`test_horizontal_glyph_grid_axes`（横向轴角色未被波及：
  列→tick、行→key，cell 落点与屏幕单调性逐项钉死）、
  `test_vertical_roll_text_tool_reaches_editing_and_confirms`（端到端：拉框 → 松手进编辑态 →
  输入文字 → 点 √ → 生成音符全部落在框的 key/tick 范围内）、
  `test_vertical_roll_text_box_drag_move_uses_key_axis`（框内拖动走 key 轴、时间轴不动）。
  横向 6 项既有采样用例改为经**生产入口** `sample_to_notes_grid` + 横向 `GlyphGrid` 跑
  同一批输入与期望值（原先的 `sample_to_notes` 兼容适配层已删除，不留测试专用缝）；
  `cargo test -p lumino-ui-editor --lib text_tool` 25 全绿。

### 音符画悬浮工具条 · 工具设置面板改「已启用后再次点击」弹出 + 面板居中对齐按钮（BUG 修复）

- **症状（交互）** — 悬浮工具条上要弹出某工具的设置面板（画刷设置 / 形状选择 / 分音符
  填充），必须**按住 Ctrl 再单击**条目：手势不可发现，且 Ctrl 状态在焦点切换等场景下本就
  不可靠（`Editor::ctrl_pressed` 需要 canvas + 窗口级双通道兜底）。
- **症状（对齐）** — 弹出的面板没有居中对齐按钮，而是贴在**悬浮工具条左端**。
- **根因（对齐）** — 设置面板悬浮层 `CurveToolGroup` 的锚点 = 它的**内容元素**。此前传入的
  内容是**整条胶囊**（宽 265），面板（宽 248）于是以"胶囊中心"居中：`(265-248)/2 = 8.5`，
  面板左缘只比胶囊左缘右移 8.5px ⇒ 视觉上就是"贴左端"；与被点的按钮中心（形状按钮在胶囊
  内偏移 205）相差 **72.5px**（实测：面板中心 700.0 vs 按钮中心 772.5）。几何层的居中逻辑
  本身没错，错的是**锚点给成了整条胶囊**。
- **修复（对齐）** — 把设置面板挂到**触发它的那个图标按钮**上：`CurveToolGroup::new(该按钮,
  面板)`（面板元素随条目在行内构造，`draw_tool_settings_menu` 返回归属条目），锚点因此严格
  等于被点按钮 ⇒ 面板水平居中于该按钮。
- **修复（交互）** — 条目**已启用**（工具已激活 / 填充已开启）时**再次点击同一条目**即弹出
  设置面板；未启用时首次点击仍只做选择。点击→消息的唯一出口收敛为
  `root/draw_toolbar.rs::tool_panel_item_press`，事件由 `ToolPanelItemCtrlSelected` 改名为
  `ToolPanelItemSettingsRequested`（名字与 Ctrl 解耦，不再保留误导性的 Ctrl 契约）。
- **补齐关闭路径** — 「再次点击颜料桶」不再等价于"再点一次即关闭填充"（悬浮条上原 toggle
  关闭入口随之消失，而主工具栏的填充开关按钮早已移除），故在分音符填充面板内新增
  **「关闭填充」**（`FillToggled(false)`：停用填充并收起面板）。另一条等效路径不变——点击
  其它绘制工具条目会关闭填充共存态。
- **明确不动的地方** — 画布上 Ctrl+单击打开分音符填充面板的路径保留（另一条独立入口）；
  画刷 / 形状下拉的"点击面板内空白即关闭"语义不变。
- **验证** — 先复现后修复：`test_settings_panel_is_centered_on_its_trigger_button` 用
  headless iced 渲染器（`iced_wgpu` + `Shell::headless`）实测布局，修复前 **FAILED**
  （面板中心 700.0 vs 按钮中心 772.5），修复后画刷 / 形状 / 颜料桶三个条目均居中（±1px）。
  另加 `test_tool_panel_item_press_contract`（逐条目钉死点击→消息映射）、
  `test_tool_settings_menu_owner`（面板归属条目）、
  `test_panel_second_click_fill_bucket_opens_panel_without_ctrl`（全程不按 Ctrl）、
  `test_fill_panel_disable_button_turns_fill_off`；`cargo test -p lumino-ui --lib` 399 全绿。

### 音符画 · 曲线/形状工具生成音符的最小长度下限（BUG 修复）

- **症状** — 纵向卷帘下画一条**垂直于时间轴的直线**（时间原地不动、一瞬间跨过多个音高
  行），生成的音符每条只占 1 tick：1920 ppq 下是 1/7680 个全音符，缩放到实机比例后不足
  1px ⇒ 看起来像「音符宽度过小 / 没生成」。
- **根因** — 轮廓音符由蜘蛛网式逐音高行解析得出（`line_tool/paths.rs`）：为避免紧折角后
  面接平坦段时两条长音符叠成实心块，同一 tick 跨多行的音符**刻意**只给 1 tick。几何层没错
  （保持 Spiderweb 口径），缺的是**写入层的可视长度下限**。
- **修复** — 写入层（`confirm_line_tool` / `confirm_shape_tool` 的轮廓腿）按最小长度下限
  补齐过短的音符（起点不动、终点外扩）：默认 **128 分音符** = `4·ppq/128` = `ppq/32`。
  形状工具描边是同一条几何腿，一并修掉。
- **修改接口（预留）** — `LineToolState::min_note_division`（`Some(x)` = x 分音符、
  `None` = 不设下限；默认常量 `LineToolState::DEFAULT_MIN_NOTE_DIVISION = 128`），换算函数
  `line_tool::paths::min_note_length_ticks`；`Editor` 侧暴露 `set_min_note_division` /
  `min_note_division`。属模式设置，跨 √ 确认 / × 取消保留。
- **明确不动的地方** — 填充腿（`fill_notes` / `chop_span` / 形状填充格点）**不套用**下限：
  其长度由用户选的切分档位与图形覆盖决定，补齐会让音符覆盖大于图形覆盖，破坏「填充显示
  == 生成音符」不变式。
- **验证** — 先复现后修复：`test_vertical_roll_straight_line_keeps_min_note_length`
  （修复前实测 1 tick vs 要求 60 tick），另加 `test_confirm_vertical_line_keeps_min_note_length`、
  `test_min_note_division_is_configurable`、`test_min_note_division_survives_confirm`、
  `test_outline_min_length_fills_short_vertical_segments`、`test_outline_min_length_is_configurable`、
  `test_min_note_length_ticks_follows_division`、`test_to_note_triple_clamps_start_and_length`；
  `cargo test --workspace` 全绿，`cargo clippy -p lumino-ui-editor -p lumino-editor-state --all-targets` 无警告。

### 音符画 · 三角形朝向跟随拖拽方向（BUG 修复）

- **症状** — 形状工具选中「三角形」拉框，画出来的永远**倒三角**，与拉框方向无关；
  预览、√ 生成音符、鼠标工具点选命中、填充四条腿一起倒。中途一度修成「永远正立」，
  同样不符合预期——**顶点必须跟随拖拽方向**。
- **期望语义（本次口径）** — 顶点始终朝**拖拽起点**那一侧的 key 边：
  向下拉（key 递减 / 屏幕向下）⇒ 顶点在高音高侧 ⇒ 横卷帘**正立**（高度为正）；
  向上拉 ⇒ 顶点在低音高侧 ⇒ **倒立**（高度为负）；拖拽过程中越过起点 ⇒ **实时翻面**
  （预览每帧重算，不必松手）；纵向卷帘转置后同一条规则（向左拉 ⇒ 顶点朝右）。
- **根因** — 不是某一侧写反，而是**方向在规范化时被丢掉**：`ShapeInstance::rect` 经
  `normalize_rect` 强制 `y0 <= y1`，朝向信息无处承载，`normal_triangle_verts` 只能钉死
  某一侧。首版（提交 `3814001c`）钉在 key 小的一侧 = 屏幕下方 ⇒ 永远倒三角；上一轮修正
  钉到 key 大的一侧 ⇒ 永远正立——两次同一个根因。（key 轴正向 = 音高更高：横卷帘
  `ViewState::key_to_y = (max_key - key)·zoom_y`（key 越大越靠屏幕上方）、纵卷帘
  `Editor::tick_key_to_pos_f32`（key 越大越靠右）。）
- **修复** — 把方向提升为**随几何走的一等信息**，而不是从外接框里猜：
  - 新增 `ShapeSpec { kind, rect, shift_constrained, apex_high }`（几何参数打包，渲染 /
    命中 / 格点 / 描边四条腿共用）；`apex_high` = 三角形顶点是否在 key **大**的一侧；
  - `ShapeInstance`、持久登记的 `DrawnShapeSource::Shape`（鼠标工具高亮 + 命中共用，
    丢了它就会把三角形画成镜像）、`ShapePreview` 都带上该字段；
    `ShapeToolState::{end_drag, preview_rect}` 按 `apex_high = 当前 key <= 起点 key` 计算
    ⇒ 拖拽中越过起点即翻面；
  - `normal_triangle_verts(rect, apex_high)` 按朝向选底边/顶点；
    `screen_equilateral_rect(..., apex_high)`（Shift 等边三角形：底边留在拖拽当前点侧、
    顶点朝起点侧延伸 h，等边高 = 底宽_px × √3 / 2，外接框仍保持规范序）；
  - 5 个几何入口改吃 `ShapeSpec` 并顺带消掉 `point_in_shape` / `shape_cells` 逼近
    clippy `too_many_arguments` 的参数膨胀；调用方由编译器逐一列出（漏一处编不过）。
- **同类一并修（Shift 正图形约束在单轴拖拽下退化）** — 规范化后的外接框在「纯水平/纯竖直
  拖拽」时有一轴位移为 0，老实现 `f32::signum(0.0) == 0` 会把算好的边长乘成 0：
  矩形被压成一条发丝、圆被压成一个点、三角形被压成零高/零宽。新增
  `constraint_scale`（零位移轴不参与取 min）与 `axis_dir`（零位移按正向处理），
  矩形/圆/三角三条腿统一；**两轴都有位移时取值逐位不变**（`min` 语义与既有测试不变），
  纯竖直拖拽的三角形改由拖拽高度反推所需底宽（h × 2 / √3）。
- **回归测试** — `editor-state`：向下拉 / 向上拉的 `apex_high` 与规范化外接框分别断言、
  拖拽中越过起点时预览朝向三次翻转、正立/倒立逐点镜像（顶点 + 描边折线 + `shape_vertices`）、
  命中判定跟随朝向、格点朝向、Shift 等边约束跟随朝向且单轴拖拽不退化。
  `ui-editor`：向下拉 = 正立（顶点屏幕在上）且描边逐音高行分布逐条断言、向上拉 = 倒立
  且为其**逐行镜像**、反向拖拽过程中预览实时翻面（松手后与最终朝向一致）、纵向卷帘
  正反两向、填充三角形宽行随方向翻到 key 60 / key 64、登记来源保留 `apex_high`。
- **行为变化（需视觉签核）** — 三角形朝向由拖拽方向决定（上下拉各得正立/倒立，拖拽中
  可翻面），含填充三角形与鼠标工具的命中区域；单轴拖拽 + Shift 现在会得到完整正图形
  而不是发丝/点。对已保存工程无影响（音符已落盘，重新绘制才走新几何）。
- **验证** — `cargo test --profile fast-release -p lumino-editor-state --lib`（322 passed）、
  `-p lumino-ui-editor --lib`（725 passed）、
  `cargo clippy --profile fast-release -p lumino-editor-state -p lumino-ui-editor --all-targets
  -- -D warnings`（0 warning）、`cargo clippy --profile fast-release --workspace --lib
  -- -D warnings`（0 warning，全仓生产代码）、`bash scripts/check-bom.sh`（通过）。
  **变异验证**：把 `apex_high_of` 改回「恒为 true」（即上一轮的"永远正立"）后，
  5 条方向用例立即全挂（`test_triangle_apex_follows_drag_direction`、正向/反向/实时翻面/
  填充方向/纵卷帘），复原后全绿 ⇒ 用例是真语义钉子而非同义反复。
  测试文件按 REF-001 纪律拆出 `interaction/shape_tool/tests_triangle.rs`（单文件 ≤ 400 行）。
  另（**仅 `--profile fast-release` 的口径差异，不是仓库缺陷**）：该 profile 下
  `cargo clippy --workspace --all-targets` 会在 `lumino-ui` 的**测试目标**上报 17 个
  E0277/E0599（`crates/editor/ui/src/view/video_clip/preview.rs:106` 的
  `#[cfg(test)] mod tests` 用 `()` 当 iced headless 渲染器）。根因是 iced 0.14 把 `()`
  渲染器整模块门控在 `#[cfg(debug_assertions)]` 下
  （`iced_core-0.14.0/src/renderer.rs:2`），而 `profile.fast-release` 继承 `release`
  ⇒ `debug-assertions = false`；仓库测试自己也写着「debug 构建下实现全部所需 trait」。
  因此**默认 dev profile 的 `cargo test` 与 CI 原样命令都不受影响，测试目标是好的**
  ——本条目前面所有 fast-release 验证均按 `--lib`（生产代码）口径执行，避免混入该差异。

### 音符画 · 形状工具描边对齐曲线工具（蜘蛛网式逐音高行）

- **存量欠债** — 音符画工具箱里两套「描边」语义不一致：曲线工具的轮廓
  （`interaction/line_tool/paths.rs` 的 `path_notes`）按几何交点逐音高行解析 → 每个音高行
  一条无缝变长连奏音符、不使用吸附精度；形状工具的轮廓却是「按 snap 网格枚举格点、
  每格一条定长音符」——陡峭边只剩零散格点、时间轴上不连续（对比报告
  `docs/spiderweb-vs-lumino-音符绘制工具对比.md` §五.3 记为 P0 之前的欠债）。
- **修复（只动描边，填充腿有意不动）** — 新增 `lumino_editor_state::shape_tool::shape_outline_path`
  （`editor-state/.../shape_tool/geometry.rs`）：形状边界 → **闭合** `(tick, key)` 折线，
  矩形/三角形取顶点 + 闭合点，圆按 `CIRCLE_OUTLINE_SEGMENTS = 64` 段采样椭圆；末点**显式
  复用首点**（`path_notes` 靠 `pts[0] == pts[last]` 逐位相等识别闭合环，重算 `cos(TAU)` 在
  f32 下未必相等，差一位就退化成开放路径多做首尾拉伸）。形状工具 √ 时把该折线交给曲线
  工具**同一套** `paths::path_notes(..., end_dot = false)`：每个音高行一条音符、起点 = 进入
  该行的 tick、终点 = 下一条起点（闭合环从最左点重启、竖直段各占 1 tick 同口径），
  **全程不再使用吸附精度**，与曲线轮廓逐项同源。
- **口径与边界** — 描边音符按**精确整 tick** 去重（起点是解析交点、落在吸附网格之外，
  沿用「除以 snap 再取整」的键会把相邻两行/两段误并成一条）；key 越界行按曲线工具同口径
  裁掉；「x 分音符」切分档位（`fill_division`）仍只作用于填充区间，对描边不生效。
- **填充腿行为不变（有意保留）** — `filled = true` 仍走 `shape_cells(filled = true)` 格点：
  未开切分 = 每格一条 snap 长音符，开启切分 = 按行合并后 `chop_span` 切分；
  `shape_cells(filled = false)` 保留为几何查询本身并在文档标注「描边已不走它」，防未来误用旧口径。
- **回归与结构** — 形状工具测试改为断言**逐音高行覆盖**而非格点数：矩形描边 10 条
  （上下边各 1 条贯通 + 右竖直边 5 条 1 tick + 左竖直边 5 条中与水平边同起点的 2 条被
  `keep_longest` 合并）、圆描边逐行覆盖且最高/最低行各 1 条、中间行各 2 条；新增「描边不受
  切分档位影响（逐条一致）」「登记音符与文档音符逐条同源」「圆描边折线闭合 + 吃屏幕空间
  Shift 约束」用例。`interaction/shape_tool.rs`（本次编辑后 708 行）按 REF-001 文件长度纪律拆为
  `shape_tool.rs`（交互）+ `shape_tool/confirm.rs`（√/× 生成）+ `shape_tool/tests.rs`（测试），
  与 `line_tool.rs` + `line_tool/` 同构。
- **行为变化（需视觉签核）** — 空心矩形/圆/三角 √ 生成的音符**数量与长度都变了**：1 tick
  吸附精度下 0..4 × 60..64 的空心矩形由 16 条 1 tick 格点音符变为 10 条（含 4 tick 贯通
  音符与角点 1 tick）。对已保存工程无影响（音符已落盘，重新绘制才走新口径）。
- **验证** — `cargo test --profile fast-release -p lumino-editor-state --lib`（315 passed）、
  `-p lumino-ui-editor --lib`（719 passed）、`cargo clippy --profile fast-release -p
  lumino-editor-state -p lumino-ui-editor --all-targets -- -D warnings`（0 warning）。

### 音符画 · 鼠标工具（框选）框选不到未 √ 的曲线/图形（BUG 修复）

- **症状** — 在音符画工具栏画完曲线/形状/笔画后切到「鼠标工具」拉框框选，图案明明画在
  画布上，却**框不中、没有选中选框、也拖不动**；点选同样落空。
- **根因** — 两阶段交互（拖动只产生待确认几何，√ 才生成音符）下，切换工具只**保留**
  产物不**固化**（`EditorState::set_tool`，故没有音符是对的）；但产物在鼠标工具下照旧渲染
  （`Editor::pending_preview_visible`，本意就是"看得见才框得着"），却**只在 owning tool
  的待确认容器里**——图形选中域 `ShapeSelectState` 里没有对应条目，框选命中集为空。
  「可见性不变式」与「可选中不变式」脱钩：看得见，抓不住。
- **修复** — 进入鼠标工具时把每件待确认几何**镜像**进图形选中域
  （`DrawnShape.pending: Option<PendingShapeRef>` + `PendingShapeRef::{Shape, CurvePath,
  BrushStroke}`）；音符仍只在 √ 时生成，镜像 `notes` 恒空：
  - 框选 / 点选 / 选中高亮 / 并集选框 / 框内拖动全部复用既有链路（渲染与命中同源）；
  - 拖动**即时写回** owning tool 的待确认几何（几何是唯一权威，镜像每帧重新导出），
    于是 canvas 曲线/形状预览、wgpu 画刷方块预览、镜像描边、选中选框读到的永远是同一份几何；
  - 平移口径与已确认图形一致（tick 钳 ≥ 0、key 钳 0..=255），曲线按相对控制柄整条平移，
    画刷走 `BrushStroke::translate`；曲线/画刷的待确认历史记一次快照（Ctrl+Z 可回退），
    形状工具无待确认几何历史；
  - Delete 删除待确认几何（不动文档、不记文档历史），同容器后续镜像下标前移，引用不错位；
  - 待确认几何尚未绑定音轨，镜像随任意非 Conductor 轨可见可选中（与 owning 预览的
    渲染规则严格对齐：Conductor 轨不渲染 → 也不建镜像）；
  - 离开鼠标工具即撤掉镜像（其余工具由 owning 预览渲染），重复切换保留选中态；
  - **镜像几何回灌** — `Editor::undo/redo` 的「待确认路径编辑历史」分支（brush/line
    `*_path()`，与当前工具无关：鼠标工具下按 Ctrl+Z 同样回退待确认几何）后同帧回灌镜像，
    并在每次鼠标工具按下前自愈一次；owning 几何整体消失（撤销回退整条路径）时镜像一并
    退场，绝不留「画不出来却框得中」的幽灵条目。
- **回归防护** — 新增端到端测试 `tests/drawn_shape_marquee_e2e.rs`（10 项，走真实
  `handle_action` + 屏幕坐标链路）：已 √ 的形状/曲线/画刷、多选并集选框 + 框内整组拖动、
  纵向卷帘，未 √ 的形状/曲线/画刷「框选 → 选框 → 拖动 → √ 落在拖动后位置」、
  「Delete 待确认几何 + 后续镜像可继续拖动」，以及「撤销待确认路径编辑后镜像同帧回灌」。

### 设置面板 · 关于页「回声洞」彩蛋（UI-006）

- **回声洞彩蛋文本** — 「设置 → 关于」页**最后一个文本内容下方**新增一行灰阶小字（12px，
  复用占位色 `background.weak.text`，不打断关于页信息层级）：进入关于页时以**打字机效果**
  逐字显示（20 字符/秒，`dt` 上限钳 50ms 防卡顿后瞬移）；**单击**后当前内容**闪烁 0.5s**
  退出（0.125s 半周期方波 = 2 个完整明暗循环），随后下一条以打字机效果进入，**索引循环**。
  文案为编译期常量（5 条，含 `🐧`），**不读配置、不依赖外部文件**、不做本地化（彩蛋性质，
  英文界面同样显示中文）。
- **连点安全（不可省）** — 闪烁期内的点击**一律忽略**：每次点击最多推进一条，且任何时刻
  最长 0.5s 必然离开闪烁相位。另两种直觉写法都会坏——「闪烁期重置计时」会被连点无限推迟
  （观感卡死），「闪烁期立即推进」会一次跳多条。已由单测锁死（闪烁内连点 50 次 ⇒ 恰好 +1）。
- **状态归属 `RootState`，不放 `SettingsPanel`** — `Root::apply_settings` 是**整面板替换**
  （`ui/src/root/editor_ops/dialog/settings.rs`）；放面板内会把闪烁中的瞬时状态搬进主窗口，
  使主窗口为一段与它无关的动画持续重绘。
- **装填点（不可省）** — 关于页**永远不是**设置面板初始页（`SettingsPanel::new` 中
  `selected_menu_index` 硬编码为 0），故打字机只能由 `Event::MenuSelected(MENU_INDEX_ABOUT)`
  显式装填；同时把裸魔数 `9` 提为 `MENU_INDEX_ABOUT` 常量。状态起始相位为 `Idle`
  （不要求逐帧驱动），否则 APP 启动后主窗口会空转重绘约 1 秒。
- **逐帧驱动（同类陷阱，与 UI-007 同源）** — 对话框 `ui_dirty` 只在「消息产生状态变更」或
  iced 返回 `State::Updated` 时置位，事件队列清空后走「仅 present 缓存帧」早退路径 ⇒
  `frame.rs` 门控纳入回声洞动画并**显式置脏**，否则动画 1~2 帧后冻结。
- **两处「不抖动」保障** — 闪烁用 **alpha** 而非隐藏控件（保留布局盒，零重排）；彩蛋行外包
  `width(Fill)` + `height(Fixed)` 容器（`MouseArea` 命中盒 = 子元素布局盒，否则打字初期只有
  1~2 个字宽几乎点不中；空串时也不塌陷）。逐字揭示按 `chars().take(n)` 截取——文案含 CJK
  （3 字节）与 emoji（4 字节），字节切片会在非 char 边界 panic。
- **🐧 字形可得性已探针实测（非推断）** — 三环链路逐环取证：① `font-kit` 探针确认 U+1F427
  存在于 `Segoe UI Emoji` / `Segoe UI Symbol` / `Noto Color Emoji`（211 个系统家族全枚举）；
  ② cosmic-text 的 Windows `common_fallback()` 首项即含 `"Segoe UI Emoji"`；③
  `cryoglyph` 将 `SwashContent::Color` 映射为 `ContentType::Color` ⇒ 彩色 emoji 可光栅化。
  附带查实：`config.rs` 默认字体写英文名 `Microsoft YaHei`，本机无该家族名（实际为 `微软雅黑` /
  `Microsoft YaHei UI`），因 cosmic-text 的 `han_unification()` 默认回退到 `Microsoft YaHei UI`
  故中文正常——与 UI-001 记录的边界一致，本卡不处理。
- **验证** — `cargo test -p lumino-ui-core echo_cave`（状态机 15 项：起始不动/逐字单调/
  有界收敛/emoji char 边界/点击冻结/极早期补全/方波相位/索引 +1 循环/**连点不卡死**/
  整轮有界/`dt` 钳制/重播/文案约束）；`cargo test -p lumino-ui echo_cave`（菜单装填→点击→
  `AnimationTick`→视图构建 7 项接线）；`cargo test -p lumino-ui-settings`（20 项，含
  「关于页不是默认页」护栏——该断言变红即提示装填点需重设计）。全仓门禁：
  `cargo test --workspace`（**2472 passed / 0 failed / 32 ignored**）、`cargo fmt --all`、
  `cargo clippy --workspace --all-targets -- -D warnings`（exit 0，0 警告）、
  `scripts/check-bom.sh`（1405 个 `.rs` 无 BOM）全绿。
  记录见 `docs/2026-10-03-UI-006-回声洞实施记录.md`（调研见同名「调研」文档）。

### 设置面板 · 关于页 Lumino Logo 与点击彩蛋（UI-007）

- **关于页 logo 与彩蛋交互** — 「设置 → 关于」页顶部新增 Lumino logo（复用 `Icon::LogoInApp`，
  **未新建面板/窗口**）：单击原地晃动一次（0.30s 衰减收正弦、±7px，用左右定宽占位器表达位移，
  行宽恒定不引起重排）；连点累计满 **15** 次触发彩蛋序列——
  **快速转动（2 圈/0.5s）→ 脱离原位（0.22s ease-out）→ 飞向窗口中轴 Y 以上随机位置 →
  匀加速砸向窗口底部（0.55s，重力按落点距离反解以稳定时长）→ 落地（播放「钢管落地」音效）
  → 渐隐消失（0.35s）**。相邻点击间隔超过 **2s** 计数清零重新累计（惰性判定，无定时器）。
- **消失状态仅进程内保持** — 用 `lumino-ui-core` 的进程级 `static AtomicBool` 承载，不写任何配置：
  重开设置面板/关于页不再出现，重启恢复。**不能**放面板状态：设置面板是独立窗口，
  每次打开都会 `Host::new_settings_dialog` 重建 `RootState`（`dialog/src/manager/lifecycle.rs`、
  `ui/src/host/builder.rs`），面板内字段必被重置。
- **逐帧驱动（关键陷阱）** — 对话框的 `ui_dirty` 只在「消息产生状态变更」或 iced 返回
  `State::Updated` 时置位；事件队列清空后 `render_iced_ui` 走「仅 present 缓存帧」早退路径
  （`host/render/ui.rs`），**只喂 `AnimationTick` 不置脏 = 动画冻结**。故 `frame.rs` 门控纳入彩蛋
  动画并显式置脏（先例：同文件播放分支），`DialogManager::update()` 既有 `redraw()` 路径即足，
  无需改动 `dialog` crate。
- **几何口径** — `window::Window` 无尺寸字段、视图层拿不到 viewport：由 Host 每帧
  `set_viewport()` 注入、路由点击消息前 `set_click_point()` 注入光标（`mouse_area::on_press`
  不携带坐标且 `on_move` 会在悬停期制造高频消息，故不用），状态机在像素域解算并钳制
  （落点仅取中轴 Y 以上、落地钳在窗口底部内）。悬浮层挂 `view_dialog`（整窗）而非设置内容区，
  坐标系与 Host 视口逐像素一致，logo 可越过自有标题栏但严格限制在窗口可见区内；
  该层不套 `mouse_area` ⇒ 点击穿透，底层设置内容照常可点。
- **旋转尺寸补偿** — iced `Svg` 以旋转后包围盒参与 `ContentFit::Contain` 缩放，不补偿时
  **非正方形** logo 转动中会周期性缩到约 77%；`ui-core` 新增 `icon::view_transformed()`（旋转 + 不透明度）  并按 AABB 长边比补偿，控件置于固定方盒居中吸收布局呼吸。附带：`SettingsPanel::update` 是
  **穷尽匹配、无 catch-all**，新增 `Event` 变体必须同步补 arm（本次已补）。
- **落地音效（内置「钢管落地」）** — 「设置 → 关于」彩蛋落地瞬间播放，音频以 `include_bytes!`
  内嵌（`resources/sounds/pipe-impact.mp3`，48 kHz 立体声 2.366s / 49,059 B），**运行期不依赖外部文件**。
  新增 `lumino-midi-io::ui_sfx`：专属工作线程 + 懒开流（播完暂停）、**音频回调零锁零分配**
  （`Arc<Vec<f32>>` + 两个原子量，沿用仓库「音频回调不持互斥量」纪律）、解码/开流失败
  **静默降级**不影响彩蛋其余流程、输出设备跟随设置面板选择。彩蛋触发瞬间 `prewarm`
  （序列 1.27s 后才是落地，足以覆盖解码+开流），落地只剩一次 `play`。
  解码用仓库既有的 symphonia，仅追加 `mp3` feature（特性合并后仍是单份构建，
  多出同族 `symphonia-bundle-mp3`，MPL-2.0 与既有 symphonia 同源）。
- **修复 symphonia 不跳 MP3 编码器延迟（换素材后仍成立）** — 未处理时内嵌素材解码后开头带
  ~23 ms 近静音（ffmpeg 会按 LAME 头跳过、symphonia 不会；旧素材 ffmpeg/symphonia 前导差
  24.9 ms、新素材探针实测裁掉 1102 帧 = 23.0 ms，两次独立素材互证），会让「落地后不即响」。
  解码后按能量阈值裁掉前导静音（保留 0.08 ms 软起振余量），与素材解耦、换素材无需改代码。
  起振判据为「达到峰值 90% 的时间 ≤ 30 ms」（实录冲击自带 ~20 ms 爬升，固定 10 ms 窗口过严），
  并经**反证**确认有效：摘掉裁剪后实测 47.5 ms、断言如期变红，恢复后回绿。
  播放增益 `SFX_GAIN = 0.6`（素材峰值 0.0 dBFS，隐藏彩蛋不宜满刻度）。
- **验证** — `cargo test -p lumino-ui-core about_egg`（状态机 11 项：计数/超时/相位时序/自然加速/
  中轴约束/越界/硬超时/消失语义/落地信号一次性）、`cargo test -p lumino-ui about_egg`
  （消息→处理器→状态→视图接线与序列终止）、`cargo test -p lumino-ui-settings`（消失后不再渲染 logo）、
  `cargo test -p lumino-midi-io ui_sfx`（内嵌素材解码/前导裁剪/重采样/声道映射 9 项）；
  另有端到端实跑探针确认设备枚举→开流→回调→二次播放全通（`UI 音效就绪：48000 Hz / 2 声道`）。
  `cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace` 全绿；新增 `.rs` 无 BOM。
  记录见 `docs/2026-10-02-UI-007-关于界面Logo与彩蛋实施记录.md`（调研见同名「调研」文档）。
- **遗留** — ① 音效素材由用户提供（曲源标注 **NCS**，使用者确认可直接使用且不要求进一步留档）；
  ② 视觉流畅度与「非动画期无常态重绘」需实机人工确认；③ 音效输出设备在首次初始化时确定，
  运行中更换设备需重启生效。

### REND-002 多端口（Phase 1：映射原语与写死解锁）

- **共享映射原语** — 新增 `multi_port` 模块：`global_channel(port, ch) = port*16+ch`（u16 防溢出）、
  `split_global_channel` 逆映射、`channels_for_max_port`、产品上限 `MAX_PORTS=16` 与
  `effective_port`（超限折叠到端口 15 块，B1 决策）；`MidiDocument::max_port()` 查询。
  单测覆盖 port=0 恒等、u7 边界、往返与折叠边界。
- **解除“写死 16 通道单 Port”** — CPU 离线导出算法零改动，仅新增端口维度并全部由
  `AudioRenderConfig.midi_max_port==0` 门控（单端口路径与历史逐字节一致）：
  ① `build_group_config` 由 `synth_format()` 决定 `Midi` 或
  `Custom{channels:(min(max_port,15)+1)*16}`；② `dispatch_event` 接收来源轨道端口，
  多端口时映射全局通道；③ 内存事件流 `MergedEvent` 与流式 `StreamingMidiPlayer`
  携带每轨 FF 21 端口（`MergedEvent` 新增字段填补原 padding，体积不变）。
- **输出接口扩宽** — `OutputConnection` 通道参数 u8→u16（全局通道）；默认实现保持
  低 4 位折叠，外部 MIDI 设备行为不变；XSynth 输出直接消费全局通道；LGS 维持 4bit
  折叠（GPU 范围 #87）。新增折叠行为回归单测。
- **范围**：本阶段不启用多端口实际映射；入口写入 `midi_max_port`、每端口 ch9 打击乐
  初始化、超限告警计数与多端口回归 fixture 属 Phase 2；实时管线格式与播放通道空间
  属 Phase 3。记录见 `docs/2026-10-01-REND-002-CPU多端口Phase1解锁记录.md`
- **CPU 导出落地（Phase 2）** — ① 流式/内存两条渲染入口按文件/文档推导
  `midi_max_port`（UI 不感知端口）；② `Custom` 播放前对每个端口 `p*16+9` 显式
  `SetPercussionMode(true)`（fork 的 `Custom` 不自动开 ch9）；③ 端口超上限（>15）
  显式告警（超限轨道清单）并在渲染结束汇总折叠事件数，不静默丢音/串台（B1）；
  ④ 新增双端口/单端口自动 fixture 与映射纯函数单测（`multi_port_render.rs`），
  CPU 离线导出算法零改动。记录见 `docs/2026-10-01-REND-002-CPU多端口Phase2导出落地记录.md`
- **CPU 实时落地（Phase 3，含运行时打击乐模态 B）** — ① `XSynthOptions.midi_max_port`
  驱动实时 `SynthFormat::Midi/Custom`，`Custom` 逐端口显式开 ch9；
  ② `Api::set_midi_port_layout` + “重建成功才提交布局”，文档装载/新建/关闭在
  runner 侧应用，异步初始化完成后对齐；③ 播放 `MidiMessage/EventType/NoteEvent`
  通道升 u16 全局通道，当前轨/其他轨/automation 均按来源轨道端口映射；
  ④ 共享 `PercussionTracker`（midi-model）按 GS/XG Bank Select 约定在导出与实时
  同步切换打击乐模态（切换消息先于触发 CC），chase 追齐模态并修正跨端口通道；
  ⑤ 暂停清理改 `release_all_dampers`（XSynth AllChannels）覆盖全部端口；
  ⑥ 单端口保持 `Midi`/恒等路径（零行为变化）；
  ⑦ 质量分析处置：文档装载强制重建以清理跨文档通道模态、GPU 多端口显式降级告警、
  端口口径收口为 `track_global_channel/channels_for_max_port_clamped/percussion_channels`、
  超 400 行文件按仓库约定拆分；复检修复重建后 `master_peak` 句柄同步、装载时序
  （停播→重建→连播放输出）与混音重下发；
  ⑧ 接口演进：基础 `Api`/`OutputConnection` 冻结，三项能力迁入扩展 trait
  `SynthControl`/`PlaybackOutput`（默认实现 + 各后端显式 impl），存储与流转改用
  扩展 trait 对象；
  ⑨ 重建耗时实测与优化：全量重建 104–188ms（缩回 16 通道最贵）→ 同布局文档切换
  改轻量复位（`SetPercussionMode(false)`×N + `SystemReset`），实测 0.012–0.114ms，
  仅端口数真变化才重建；
  ⑩ P0 维护批：未钳制 `channels_for_max_port` 收为 `pub(crate)`、折叠事件统计口径
  统一、内存渲染进度显示真实音符数、`AudioRenderConfig` Debug 补全、chase 后触契约
  写明、`fade_out_killing` 注释补 fork 引用（原注释正确，核实为误报）；
  ⑪ 导出限幅器修复（验收发现，独立缺陷）：包络 attack 由 10ms 改瞬时 + 非有限样本
  按静音处理 + 状态有限性防御 + 非有限样本计数告警——修复“限幅器开启但峰值
  1.5–30×”“首个 NaN 后限幅器永久旁路”；上游 NaN 源待归因；
  ⑫ 导出数值污染兜底与取证：非有限样本无条件净化（无论限幅器开关）+ 一次性
  NaN-PROBE（首个非有限样本时间 + 最近 64 条事件）；隔离实验确认触发需
  PB ∧ CC6/38 ∧ RPN 选择三者组合，根因指向 fork 合成侧；
  ⑬ 根因修复（方案 A）：fork xsynth-Lumino `7a31ed2c`（process_pitch 有限性
  钳制 + 采样器 time 守卫），Lumino pin 已更新；修复后复现实验 nan=0（修复前 19890）。
  记录见 `docs/2026-10-01-REND-002-CPU多端口Phase3实时落地记录.md`
  ⑭ 同 tick 控制事件次序【决策，2026-10-03】：**保持文件序，不引入「选择→数据」二级序**。
  素材把 RPN DataEntry 写在选择之前（审计 803 组）属素材侧问题，解析/合并层不「修复」；
  加二级序会同时威胁 PB 的相对次序，回归 `426d5b2d` 修好的弯音偏差（98k PB 中 7 个偏差
  >1 半音、最大 62 半音）。追齐路径（`chase.rs`）仍用「选择→其他→DataEntry」rank——
  那是在重放下发「某 tick 的最终状态」、次序由代码语义决定，与「忠实还原文件流」是两件事，
  差异有意为之。决策由 `event_stream::tests::test_same_tick_rpn_data_before_select_is_preserved_as_is`
  锁死（改语义必须先让该用例变红）。
  ⑮ 通道后触误映射修复：`XSynthOutputConn::send_raw` 曾把 `0xD0`（通道后触）映射为
  `ControlEvent::Raw(0, b1)` = **CC0（Bank Select MSB）** ⇒ 后触会改掉该通道音色库；
  多端口下默认实现还会把全局通道折叠到低 4 位 ⇒ 改错端口的同号通道，并污染
  `PercussionTracker` 的 Bank Select 判定（误切打击乐模态）。现抽出纯映射函数
  `map_raw_message`：`0xD0`/`0xA0` 一律**显式不支持**（无声丢弃，与 `lgs.rs` 既有决策
  「GPU 合成器不支持，忽略以避免噪声报错」一致），并显式覆写 `channel_pressure`/
  `poly_pressure` 绕开默认折叠路径；附回归守卫（反证：恢复旧映射该用例即变红）。
  附带澄清两处口径：`set_channel_gain/pan` 的 `u8` 形参**不是**缺口（全局通道上限 255，
  恰好覆盖全 256 通道）；`get_channel_levels` 固定返回 `[f32; 16]`，多端口下只覆盖端口 0，
  已在 trait 文档明确记为**已知限制**（未修）。

### 渲染修复

- **重叠音符逐帧闪烁修复** — 钢琴卷帘重叠音符（同轨同 key 叠音、预览与已有音符重叠）的深度只编码到轨道粒度，配合 `cull.wgsl` 抢占式 `atomicAdd` 导致的可见顺序随机，管线 `LessEqual + depth_write` 下「同深度后画者胜」的赢家逐帧变化，重叠区描边抖动。四份音符 shader（含纵向转置版）改为「预览 0.0 / 主音轨 2^-17 / 洋葱皮 (track_enc+1)×2^-16」显式分层，并以 chunk 内源索引（跨帧稳定）注入基深度尾数低位做轨内确定性平局裁决；同轨最大偏移恒 < 2^-16，不侵占相邻轨道层、不越远平面（`bbca7239`）
- **导出路径 depth 兼容性** — 视频导出的洋葱皮渲染器此前沿用带 depth-stencil 的管线，与无 depth attachment 的导出 RenderPass 不兼容（一旦上传实例即触发 wgpu 校验错误并丢弃整条命令缓冲）；新增无 depth 变体并同步导出渲染器集合（`7879d75e`）
- **验证补齐** — 新增深度精度预算单测（严格单调 / 不越层 / 不越远平面 / shader 契约一致）、64 帧绘制顺序无关的像素级回归、无 depth 路径覆盖掩码等价与管线兼容性测试（`9069dbae`）
- **CI 测试稳定性修复（深度能力门控 + 挂死根治 + 测试设备共享）** — ① 深度像素测试新增「适配器深度分辨率探针」能力门控：软件光栅器（CI ubuntu Mesa 软渲染）在 NDC 变换/定点深度下把亚 2^-24 深度全部压平，探针不通过时打印明确原因跳过细粒度深度断言（真实 GPU / WARP / Metal 全量断言，管线错误仍以 wgpu 校验 panic 暴露）；② 根治测试挂死：全 crate 14 处 GPU 回读 `poll(Wait, timeout: None)` 无限阻塞改为非阻塞轮询/有界等待（含导出 staging 生产路径），消除「deadline 形同虚设」类挂死；③ 测试 GPU 设备改为全 crate 单设备共享（`test_gpu`，避免并行测试多设备并发导致驱动停顿），套件耗时 ~8s → ~3.3s，连续 6 轮全绿（`616a4f8c`）
- **深度索引饱和/跨 chunk 别名修复（黑乐谱加固）** — 用项目自有 midly 链路实测黑乐谱（19,360,995 音符 / 31 轨，单轨最高 494 万）发现：旧「ulp 预算」方案的主轨 629 万 / 洋葱皮 209 万索引预算被真实规模打爆，饱和索引共享同一深度 → `LessEqual` 同深度后画者胜的赢家随 cull 顺序逐帧随机 → 单轨叠音闪烁回归；且旧方案用 **chunk 内局部索引**，多 chunk（设备 binding 上限/16）时索引重置造成深度别名。四份音符 shader 改为「区域化位空间映射」：预览 0.0 < 主音轨区 `0x00800000` < 洋葱皮区 `0x1F800000`，区内 `bits = 区起点 + 全局索引`（`chunk_start + 本地可见索引`，经渲染 bind group 新增 binding 3 复用 cull uniform 槽位折叠），每区 5.2 亿槽位覆盖项目 2.9 亿目标；区顶恒 < 远平面。回归证据：黑乐谱规模索引全程唯一、chunk 折叠构图验证（后画 chunk 必须更靠后）、64 帧像素一致继续通过（`37398bea`）
- **切工程卷帘崩溃修复（`note_cull_bind_group` invalid）** — 空白工程建几个音符后打开 MIDI 文件时卷帘崩溃。根因：洋葱皮全量会话（`BeginSession → Chunk → Done`）边界只清了 GPU 实例计数，未清零 `NoteRenderer::last_upload_count`——会话进行中（`Reserve` 已换新 buffer、bind group 尚未重建）渲染帧仍按旧计数调度 compute/draw，`set_bind_group` 拿到与新 buffer 不匹配的陈旧 `note_cull_bind_group`，wgpu 校验层把错误延迟到 `Queue::submit` 才爆发（`STATUS_CONTROL_C_EXIT`）。四层修复：① `begin_streaming_upload` 同步清零 `last_upload_count` / `external_bound`，会话中帧直接早退不触碰陈旧 bind group；② `prepare_pass` 按 `min(chunk_count, cull_bind_groups.len())` 调度并告警降级（与 `draw.rs` 既有策略拉齐，消除该处唯一的无守卫索引）；③ cull/render 建组前拦截空 chunk（`chunk_len == 0` 使 `size` 退化为 `None`＝整 buffer 语义）与 `offset+size` 越界，跳过并打印 chunk/offset/容量上下文便于定位；④ `ChunkLayout::from_limits` 将 `instances_per_chunk` 向下取整到 `slot_align/4` 倍数——可见索引 buffer 按 u32（4B）切片，`start*4` 此前不满足 storage offset 对齐（2GB-1 上限下 134 217 712 × 4 % 256 = 192），**多 chunk（黑乐谱规模）时 chunk≥1 建组即 invalid**，属本次一并根治的同类隐患。回归：`cargo test -p lumino-gfx --lib` 179 passed（含新增可见索引切片对齐断言），`cargo fmt --check` / `cargo clippy -p lumino-gfx --lib` 通过（`9dc5f0c3`）

### 渲染性能

- **VS cull 直绘：UI 路径取消 compute cull + 洋葱皮改不透明（PREF-005，含可见占比闸门）** — UI 渲染路径的主音符层不再跑 compute cull：新增 `onion_note.wgsl::vs_direct` 入口，源索引取自 `@builtin(instance_index)`、视口相交判定与 `cull.wgsl` 逐字同口径（不可见 / 静音轨 / `length ≤ 0` 一律退化为视口外零面积点，**不引入任何 LOD 剔除**）；每 chunk 一次普通 `draw(4 顶点 × chunk_len)` 取代 `draw_indirect`，启用时不再跑 cull dispatch（改 `prepare_direct`，只写相机 uniform）。洋葱皮管线（横纵两版）由 alpha 混合改为替换写入（FS 恒 `alpha = 1.0`）；预览层 `note.wgsl` 有 70% alpha 哨兵分支，保持混合。深度语义逐位不变（`global_index = chunk_start + instance_index` 与 cull 路径写入可见缓冲的 `local_index` 同值，`instance_index` 升序即 `region_depth` 近→远序）。**新增 `RenderParams::vs_cull_mode` 严格闸门**：真机实测放大了卡内一处量级误判——「VS 全量读 ≈ compute cull 全量读」不成立，前者是**顶点着色器为全部实例各跑一遍**（≈0.65 ns/实例），后者是**一趟 compute**（≈0.10 ns/实例）后只画可见实例；zoom-in 档（1600 万音符 / 1.5 万可见）实测直绘 **0.87→5.82ms、1.39→10.89ms（慢 5~8 倍）**。由 `0.65N < 0.10N + 0.65V` 解出直绘占优条件 `V > 0.85N`，故 UI 侧按「可见 tick 比例 × 可见 key 比例」估算占比、> 0.9 才启用（纯函数 `estimate_visible_fraction` + 3 个单测：全曲开闸 / 放大关闸 / 退化输入保守关闸）。导出路径无 depth ⇒ 始终走原 compute cull，双路径共存；因两条路径已证明**逐位像素等价**，闸门切换零视觉风险
- **PREF-005 真机 A/B（新增 `perf_tests::bench_vs_cull_ab`，含顺序偏置控制）** — 三条路径对比：混合+compute cull（改动前）/ 不透明+compute cull / 不透明+VS 直绘；**两种视图档位（全景 / 放大）× 两种负载形态（同 key 堆叠 · 黑乐谱形态 / 均匀铺满）× 多档规模**，15 轮中位数 + **正逆序双向轮转取最小值**（单轮顺序采样会把 dense 8M 档测出「40% 提速」的顺序假象，实测已复现并修正）。**实测结论与卡片归因不一致，必须记录**：① 全景档总降幅稳定在 **8~12%**（dense 11/12/11%，uniform 9/8/10%），**未达卡内「≥50%」**；② 收益恰好≈省掉的 compute cull 本身（16M 全景早先实测 cull 1.60ms / 总 15.35ms ≈ 10%），early-Z 的额外收益在本次测量中不可见；③ **根因①「alpha 混合空转并抑制 early-Z」被否证**——去混合单项 ≈ 0%（-4%~-1%，噪声内），NVIDIA 上 alpha blending 不关闭 early-Z（blending 属 ROP 阶段，与深度测试正交），blend ROP 的读改写开销在此负载下可忽略；④ **`zoom-in 不劣化` 的假设被证伪**（慢 5~8 倍，见上一条），该档最终靠闸门保住不回归。不透明管线仍保留（FS 确实恒不透明，语义更正确、零回归）
- **VS cull 直绘的正确性契约（新增 `direct_tests`）** — ① **逐位像素等价**：同一重叠加叠场景（主轨三音同 key 互叠 + 两轨洋葱皮 + 一轨静音）在「compute cull + 可见索引」与「VS cull + 实例序直绘」两条路径下渲染结果**完全相同**（常规缩放 + 亚像素全景两档），这是「改绘制序零视觉变化」唯一可信的验证方式（REND-001 `tie_break_depth` 是其前提），并附反假绿守卫（常规档位必须真有覆盖）；② `vs_direct` 自判可见与 `cull.wgsl` 同口径：视口外 / `length ≤ 0` / 静音轨一律零片元。另补 `perf_tests` 的真机分段基准（见 PREF-004 条目）

- **标尺小节号定位 O(小节数) → O(段数)（PREF-004 P2）** — `grid/ruler.rs::MeasureIterator::advance_to` 原本从 tick 0 逐小节 `step()` 推进，而横向滚动时 `Editor::set_scroll_x` 每帧失效标尺缓存 ⇒ **每帧**重跑一遍定位，成本 = O(起点之前的小节数)。改为先定位拍号段、再按「段内小节长度恒定」闭式算出小节号与边界 tick（跨段边界沿用原 `step()` 的跳转语义：段尾不满一小节也计 1 小节）；注意 `ts[0].tick` **不**参与段 0 起点定位——旧实现 `ts_index = 0` 时从 tick 0 起步，`ts[0].tick` 只通过 `time_signature_at(0.0)` 参与初始拍号选取，这是逐位等价的前提。**等价性用「把旧算法原样留作参考实现」的测试锁定**：6 组拍号配置（含空表 / 首拍号不在 tick 0 / 多段切换 / 两档 ppq）× 600+ 探测点（均匀采样 + 每个段起点及其 ±1 tick）× 比对「落点 tick + 小节号 + 其后 8 个小节边界」；另有 1e9 tick 跨度的常量时间守卫（旧实现需约 52 万次 step）。典型工程（数百小节）收益可忽略，**真实价值在病态 tick 跨度文件**——旧实现每帧几十万次步进，属可直接冻结 UI 的量级
- **P2 其余两项经量化后判定不做（记录理由，避免后人重复投入）** — ① 「`set_scroll_x/y` 逐事件 `invalidate_caches` 是固定浪费」不成立：`invalidate_caches` 只是 `canvas::Cache::clear()`，**O(1) 且幂等**，两次 draw 之间 clear 一次与 N 次完全等价（重建只在 draw 时发生一次）⇒ 收益 0；而「拖拽期间不 clear」会画出错位标尺/键盘，是负收益。② 拆分 `update_cull_info` 的 bind group 重建：该重建只在**编辑帧**（主轨事件）与整轨上传发生、**不在滚动路径**，每次 2 套 × chunk 数 ≈ 40~120µs，折算 ≈0.04~0.12ms/帧；且当前 bind group 切片长度按实例数计算，要「只在容量变化时重建」就得改成按容量切片绑定、放弃一层边界保护换 0.1ms，性价比不成立（P0 已消除唯一的每帧重建路径）

- **音符层 GPU 分段基准（PREF-004 P1 决策依据）** — 新增 `note_renderer/perf_tests.rs`（`#[ignore]`，`LUMINO_PERF_NOTES` / `ITERS` 可配），把每帧 GPU 成本拆成 clear / cull pass / render pass / 点图元直绘四段独立计时（各段单独 encode → submit → 有界 `poll(Wait)`，取多轮中位数、前 2 轮预热丢弃）。**真机实测（RTX 2060 / Vulkan / 1920×1080）推翻了卡片与我此前的估算**：全景下 cull 只占 5~10%（16M 音符：cull 1.60ms vs draw 13.44ms），且 draw 的带宽占用仅 19 GB/s（峰值 336 GB/s 的 6%）⇒ 瓶颈既不是 cull 读取带宽也不是 VS 二次读，而是**每实例的固定图元处理成本**（≈0.75~0.85 ns/实例，与 quad 面积无关；卡片所用的 `Gpx/s` 像素率模型量纲错误）。据此把 P1 靶子从「cull 范围裁剪 / 去掉 VS 二次读」改为「减少实例数（密度聚合）」，并记录外推：1.5 亿全景 ≈ 112~130ms/帧，距 T ≤ 16.9ms 差约 7 倍
- **亚像素档位点图元直绘（PREF-004 P1）** — `zoom_x × 吸附精度 < 1px`（最细可画音符也不足 1 像素）时，主音符层改走 `PointList`（每实例 1 顶点 1 点）并**跳过整趟 cull pass**（可见性判定移入新的 `vs_point`，省掉全量读 + 可见索引写）；实测 **1.22~1.28×**（16M 全景 15.35ms → 12.00ms）。深度契约不变（`global_index = chunk_start + instance_index` 与 quad 路径同值 ⇒ 逐位一致，重叠音符稳定裁决不回退）；无点管线的渲染器（预览层、导出无 depth 变体）自动回退 quad 路径，不静默不画。判据位于 UI 侧 `build_render_params`，写入新增的 `RenderParams::subpixel_note_mode`（默认 false，纵向卷帘不生效）。**行为变化（需视觉签核）**：quad 路径在亚像素宽度下 `horiz_margin` 趋于无穷，整块音符被判成描边色（原色 ×0.4，明显偏暗），点路径改为纯色 ⇒ 全景画面会更接近真实音轨色
- **滚动条拖拽全程卡顿（黑乐谱场景）— present 路径与渲染线程解耦** — 千万~亿级音符工程拖拽滚动条期间 UI 持续卡顿的根因不是滚动事件风暴（iced 已把一次事件循环内的全部事件批量投给同一次 `ui.update()`，一次 update 只产生一个 `redraw_request`），而是 UI 每帧 present 都先与渲染线程强制会合：`render_with_separate_thread` 无条件 `wait_for_frame`，而离屏纹理单缓冲、与 UI 共用同一条 command queue，UI 每 present 一帧就携带一次全量音符渲染提交，交换链又允许 CPU 领先 2 帧 → GPU 队列常驻 2~3 帧全量渲染，输入到光子的内容延迟被放大到 2~3 个 GPU 帧。三处收紧：① `RenderParams` 新增 `content_dirty`——仅音符编辑增量 / 洋葱皮全量或增量重传 / 预览实例变化 / 视口尺寸变化（离屏纹理重建）置位，此类帧保持 `wait_for_frame` 同步呈现，「音符放置后不立即显示」竞态不回退；纯视口变化帧（拖拽滚动 / 缩放）跳过等待直接 present 最近完成的离屏纹理，画面最多落后一帧、数据正确性不受影响。② 新增 `Flush` 收尾帧（`pending_present_flush`）：视口稳定后不再堆叠渲染、只同步呈现已提交帧——没有收尾时「单次 Ctrl+滚轮缩放 / 拖拽最后一帧」这类只产生一帧的视口变化会永远停在离屏纹理里（表现为滚轮无反应）；是否登记由 `RenderViewKey` 视口指纹（scroll / zoom / 画布 / 视口 / 力度面板矩形等全部绘制输入标量）判定，呈现后清除，循环必然收敛。③ 交换链 `desired_maximum_frame_latency` 由 2 收紧到 1（`LUMINO_FRAME_LATENCY` 可覆盖，供真机 A/B）——这是唯一能真正限制「UI 领先 GPU 多少帧」的杠杆。**刻意不做**「按在途帧数跳过提交」：`wait_for_frame` 的完成信号是**提交序**而非 GPU 执行完成（渲染线程全程无阻塞式 `device.poll(Wait)`），用它做闸门信号恒为 0、形同空转，故背压交给交换链。附带修复：预览实例未变化时不再逐帧下发 `PreviewInstances`——原先每帧都重传实例缓冲并触发 `NoteRenderer::update_cull_info` 无条件重建 cull/render 两套 bind group（铅笔工具 hover 预览、拖拽滚动时每帧 2×N 个 bind group 创建）；`WgpuRenderThread::send_params` 的「通道满则丢弃旧帧」注释更正（`std::sync::mpsc::channel` 为无界通道，积压合并发生在渲染线程 `process_commands` 的 drain-to-latest）

### 钢琴卷帘框选精度与边界口径

- **框选 X 向按「音符精度」量化（单元覆盖式）** — 框选 X 向此前使用鼠标原始 tick，不跟随工具栏「音符精度」，低精度模式下框缘判定不可预测。改为新增 `ViewState::snap_marquee_anchor` / `snap_marquee_edges`：按下时记录锚点单元低边，拖动期间两端由 `(锚点, 鼠标 tick)` 统一推算（右拖 = 锚点低边 → 鼠标单元高边；左拖反向），由此**两端恒落格线、方向保持、锚点固定可回缩**（向左拖再拖回右侧不会单向扩张）。明确禁用单侧 `snap_tick`(floor) 刷两端——大端内缩漏选、小端反向时外扩整整一格（即历史 `snap_tick_forward` / floor 坑，评审勿改回）。指针 / Y 向框选 / 橡皮擦 Shift / i2m 四条链路统一口径；Spring 弹簧动画目标改用同一量化，保证动画帧内「看到的框 = 命中的框」；文字工具框为**生成框**（边界会成为生成音符位置），刻意保留 round 吸附并加注释区分两种语义（`c1473c42`）
- **框选命中改半开区间，框缘贴边音符不再误选** — 命中口径由闭区间改为 tick 轴半开 `[min_tick, max_tick)`：起点恰等于框右边界、终点恰等于框左边界的音符不再入选（防低精度下误选框缘音符），内部交叠的长音符仍入选；key 轴维持闭区间「整格覆盖」语义。实现上**不改共享原语** `NoteSpatialIndex::update_query`（播放头活跃判定以零宽闭区间 `update_query(tick, tick, ..)` 取音、视口裁剪同样依赖闭区间，半开化会漏音符），而是新增框选专用入口 `update_query_marquee`（遍历骨架以 const generic 携带口径参数，两种口径各自单态化，热路径无额外分支）；`drag::selection::marquee_hits` 收敛为全项目框选口径唯一来源，索引路径与无索引窗口扫描兜底双路径一致，防「工程规模不同 → 框选结果不同」的口径分裂（`b5f7fa5f`、`47333bec`）
- **框选增量差集两处正确性修复** — ① `rect_subtract` 的 key 轴 strip 边界 off-by-one：key 为闭区间时上/下 strip 未做 ±1，与 inner 重叠一行，该行已选中音符被剔除且新增路径不会补回（交集不属于 `new − old`）→ 框缩小时边界行音符静默失去选中；② 跨 remove 薄条且同时与新选框重叠的长音符被误剔，改为在 `apply_selection_delta` 按新选框二次过滤（`remove_list.retain`，O(|remove_list|) 次零 clone 查询，与既有剔除循环同阶）（`fafff384`）
- **回归与决策台账** — 新增三组框选测试：命中口径（半开边界、跨边界长音符、key 轴闭合、零宽框、索引/窗口双路径逐索引一致）、精度契约（按下/移动/反向/锚点回缩、四档精度 1/4~1/32、Spring/橡皮擦/Y 向工具、端到端格线贴边音符不入选）、增量不变式（7 步选框序列的增量结果逐步等于对最终选框做全量重建的结果，含防空集假绿的反向守卫）；Core 层量化原语另配纯数学单测（锚点 floor、四档精度两端恒落格线 + 单元覆盖性）。②③ 两项修复经蓝军自证：临时撤销源码改动后对应用例全部 FAILED，恢复后全绿。PPQ 变更后 `snap_precision` 按工具栏精度重算并测试（`Custom` 精度为绝对 tick 值，PPQ 变更时豁免）；i2m 框选端到端同口径测试（历史为「起点 floor + 终点原始 tick」混合口径，本卡统一，属**有意偏离**原卡「i2m 豁免」条目，待评审签字）；走带 `arrangement::interaction::geometry::snap_tick` 维持 round + 拍号感知并以测试固化现状（`92bab764`）

### 其它

- **版本号更新** — 升级项目版本号至 0.2.1
- **editor-state 测试修复** — 清理 MIDI 文档测试中已被 note-id 移除重构删除的 `next_note_id` 字段，恢复 `cargo clippy --workspace --all-targets` / `cargo test --workspace` 可编译（`5c3d8b0b`）

## [0.2.0] - 2026-08-16

### midiplayer 与贴图瀑布流

- **贴图瀑布流提取** — 将高精度贴图洋葱皮系统从 gfx 依赖链中剥离，新建 `midiplayer` crate（`lumino-midiplayer`）；贴图瀑布流作为其首个功能模块（`texture_waterfall`），为后续 MIDI 播放、实时可视化等功能预留扩展空间（`abd4a78c`）
- **全面改名** — 贴图瀑布流公共 API 全面改名：`HiRes*` → `TextureWaterfall*`、`TileCoord/GroupTile` → `WaterfallTileCoord/WaterfallGroupTile` 等；`OnionSkinNote` 迁入改名 `WaterfallNote` 并删除 `onion-skin` crate（`6f9ab06c`）
- **gfx 薄化** — 命令适配层迁入 midiplayer（`WaterfallCommand`/`WaterfallGpuCtx` 与宿主解耦），`ControlCommand` 五条命令合并为单变体转发，删除 gfx 侧 695 行适配代码与全部 `HiRes*` 内部类型（`29eb7e21`）
- **兼容性说明** — 设置持久化字段（`hires_onion_enabled` 等）与对应设置事件保留原名，保证既有用户配置文件与设置面板兼容

### 云存储

- **云存储基础设施** — 新增 `cloud-storage` crate，支持 FTP/SFTP/WebDAV 协议（`dce6ab8b`）
- **云面板全流程** — 文件菜单新增云存储入口，实现云连接面板与文件浏览面板，打通云导入/保存到云全流程（`9ac4e7a3`、`b664f164`）
- **云管理设置页** — 设置面板新增云管理页，实现断连提醒与状态标志；应用启动自动连接云存储并清除提醒标志（`72f251af`、`0cc2fd97`）
- **云文件管理器** — 支持复制/剪切/粘贴/删除/重命名，修复暗色主题按钮文字（`d28cb4be`）
- **云保存与回传** — 保存到云改为单文件归档，从云导入工程自动加载到编辑器，完善文件保存流程支持云端自动回传与关闭保护（`8a4f3884`、`0d2e2dbe`）
- **传输进度悬浮窗** — 拆分云保存逻辑并新增传输进度悬浮窗（`21fd7db0`）
- **WebDAV 修复** — 修复路径特殊字符编码问题，增强连接诊断与 HTML 响应处理，修复含 HTML 实体的 PROPFIND 响应解析失败（`4db5f89a`、`e6f618e4`、`ade93207`）
- **上传修复** — 修复 SFTP 上传新文件缺少 CREATE 标志、SFTP/FTP 上传路径基准错误、FTP 路径残留与空格文件名、切换文件夹被弹回根目录等问题（`74e92319`、`ac1de552`、`76e4424c`、`b1b64ea7`）
- **云面板问题修复** — 修复云面板主题/高度/状态同步/页面错位/关闭 panic 五项问题（`067d610c`）

### 素材库

- **素材库面板** — 添加素材库面板与素材拖拽放置功能（`37b49b97`）
- **素材交互** — 修复素材拖出失效并优化拖放交互（`e1cfc6ab`）
- **悬停提示** — 悬浮提示显示名称、作者、位置、轨道数和来源，保留素材作者署名，并加入内置素材（`1c8446c1`）
- **右键菜单** — 支持素材库右键菜单重命名、删除和上传到云，重构删除确认交互（`31f03705`、`3617a7bf`）
- **列表显示重构** — 重构素材列表项显示与悬停提示，简化冗余名称显示（`24e37366`、`60f06ece`）
- **内置素材资源** — 添加多个双向 128key 及圆形材质，删除 Materials.7z 资源并修复二进制资源换行转换问题（`216a4b7f`、`b7e61a29`、`bdac4d70`）

### 图片转 MIDI

- **右侧栏 i2m 面板** — 添加右侧栏面板支持图片转 MIDI 功能，重构按钮样式并修复布局嵌套（`ba0a386e`、`fb2abc93`）
- **i2m 交互统一** — 移除独立面板切换按钮，统一图片转 MIDI 交互，重构侧边栏 UI 与 i2m 悬浮按钮图标（`2203df49`、`fcf32d70`）
- **文件选择与展示** — 完善图片转 MIDI 面板的文件选择与展示功能，优化右侧面板路径显示和标题样式（`ec9454bb`、`6352e809`）
- **放置功能** — 添加图片转 MIDI 放置功能，写入前 round 非整数 tick/长度消除亚 tick 数值（`c6de9563`、`26ecd741`）
- **配置面板** — 添加图片转 MIDI 配置面板（`afd37eda`）
- **修复** — 修复第二次 i2m 时已放置主轨音符消失、区域框拉伸塌缩与越界绘制、按钮切换面板逻辑错误、取消放置时未彻底退出放置模式等问题，移除冗余 Toast 提示（`ad576961`、`17df6c7f`、`96e1ea8d`、`9093c23b`、`a730660f`）

### 绘制工具

- **贝塞尔曲线工具** — 将直线工具重构为贝塞尔曲线路径，添加曲线工具直线绘制模式，支持自动维护贝塞尔控制柄并保持直线条件（`bafaea21`、`97a2064c`、`f8025d2b`）
- **批量路径绘制** — 支持多条路径批量绘制与独立撤销历史（`de647be8`）
- **颜料桶填充重写** — 填充改为编辑层暂存、确认时合并生成实心音符；算法从网格 BFS 改为闭环区域身份判定与矢量绕数判定，重构为延迟计算模式（`72d99a82`、`378c5383`、`3ea6ae10`、`e161e273`）
- **填充边缘修复** — 使用半格容差覆盖判定修复填充边缘缺口（`8dd851c9`）
- **图标更新** — 更新油漆桶图标为 Font Awesome 版本（`11352d73`）

### 框选、幽灵拷贝与编辑交互

- **幽灵拷贝** — 添加幽灵拷贝和 Ctrl 拖动拷贝功能，重构幽灵 delta 处理并优化编辑器复制行为（`8dd26414`、`849b9b79`）
- **复制选择框修复** — 修复音符移动和复制时长度丢失及选择框偏移；复制时选择框独立覆盖副本，不再包含原件，复制模式下只保留副本框选（`6e237382`、`a7d41279`、`b97a3ff9`）
- **框选精度** — 统一框选框精度为吸附模式，框选边界精确跟随鼠标 tick，正向拖动时 1/4 单元提前吸附避免粗精度下扩展滞后（`cd1a40fc`、`766740b2`、`6e74a948`）
- **框选样式统一** — 统一框选框样式为灰色 3px 边框与浅灰半透明填充（`f1934dd3`）
- **批量操作修复** — 批量拖动按 BPM 时序播放预览音、修复 key 上下移动无声反馈；修复 Ctrl+拖动批量复制上下移动不写入内存；统一滚轮缩放逻辑并修复批量删除选中音符（`4a7c10f4`、`dee690e7`、`255d9ccc`）

### 弯音与自动化编辑

- **弯音贝塞尔编辑** — 添加弯音贝塞尔路径编辑与确认模式，修复弯音路径交互与事件移动逻辑（`72ce4ccd`、`35463a06`）
- **曲线回环钳制** — 钳制弯音控制柄防止贝塞尔曲线回环（`6eb35b02`、`d3aa92ce`）
- **锚点交互** — 修复弯音锚点创建与拖拽交互问题，支持弯音跳变对同 tick 双锚点共存，修复跳变对锚点连续拖动时 Move 消息匹配失败（`27128f84`、`fe86b167`、`f9eb4224`）
- **工具交互统一** — 更新自动化面板工具交互逻辑，统一由 Curve 工具负责编辑（`3735fd76`）
- **Tempo 上限可配置** — 支持可配置的 Tempo BPM 绘制上限，修复 BPM 上限被硬编码截断的问题，将自定义 BPM 弹窗提升至设置对话框顶层（`68e46fb3`、`fdc47b33`、`c89393d2`）
- **双向滚轮滚动** — 支持双向滚轮滚动：水平移动时间轴，垂直滚动自动化曲线（`4e214603`）
- **样式调整** — 调整速度曲线锚点样式（`f4447d74`）

### 视频导出

- **NoteCounter 渲染模式** — 添加 NoteCounter 渲染模式及相关配置，计数器字体支持系统/自定义 TTF（`34626501`、`09dd5778`）
- **数据曲线渲染模式** — 添加数据曲线渲染模式，修复数据曲线指标始终为零的问题（`2eec9f19`、`38d02d40`）
- **导出时间估算** — 使用真实墙钟时间替代渲染帧数估算已用时间（`7d132097`）
- **超长音符修复** — 修复超长音符在导出时半路消失的问题（`8eecfd92`）
- **跨平台修复** — 修复跨平台编译时调色板资源路径问题（`08ddb909`）
- **署名合规** — 为 MIDIGraphRenderer 移植添加 MIT 署名并更正原作者（`223d5f00`、`a007ac5b`）

### 性能优化

- **ChunkedList 存储** — 将音符和控制事件存储从 Vec 迁移到 ChunkedList，历史记录使用浅拷贝和增量 CreateOp 替代整轨快照（`90c5468a`、`826c769a`）
- **超大型工程优化** — 优化超大型工程音符插入与查询性能，添加音符结束 tick 增量缓存以优化走带视图滚动性能（`eacc47f5`、`d84f7142`）
- **高密集度排序优化** — 使用计数分桶优化高密集度音符排序与回溯性能（`9cb3d67c`）
- **无锁播放帧回调** — 引入无锁播放帧回调，消除 UI 与播放线程的锁争用；修复编辑后播放游标跳回与文档快照冗余同步（`9d8b1713`、`4e1d6c3a`）
- **GPU 上传优化** — 优化标尺渲染性能减少 GPU 上传开销，移除 GPU 内存限制并实现流式上传，洋葱皮增量判断豁免非当前音轨的全量重建上传（`834dc413`、`9fea1eb3`、`4f5090f8`）

### 渲染与特效

- **洋葱皮渲染** — 添加洋葱皮渲染支持，并迁移至分离渲染线程（`0cff6645`、`bc4fce7a`）
- **音符渲染统一** — 统一音符渲染风格并修复洋葱皮调色板响应，重构音符渲染对齐 wasabi 实现（`92d727ea`、`13701760`）
- **GPU 增量事件系统** — 添加主音轨 GPU 增量事件系统以优化卷帘编辑渲染性能，添加洋葱皮事件级增量上传协议（`f1c36077`、`dc8bd46a`）
- **MIDITrail 光晕环动画** — 实现 MIDITrail 光晕环动画，调整音符光环常态半径比例（`4e55466e`、`dd8baf4a`）
- **渲染修复** — 修复重叠音符闪烁并添加稳定深度渲染逻辑，修正瀑布流音符颜色与 shader 颜色解包的通道顺序，为非预览音符添加深色边框样式（`a07f1bcb`、`da311c7a`、`44f1c577`）

### 工程与导出

- **作者字段与拍号吸附** — 添加作者字段并支持拍号变化下的小节吸附（`55b62c11`）
- **累计创作时间** — 持久化累计创作时间（`61577e3e`）
- **保存修复** — 修复保存后速度点丢失、新工程 PPQ 未同步到工程文件（落盘默认 480）、工程设置跨工程残留、同 key 音符 FIFO 配对与稳定排序等问题（`3d7d5907`、`37e2b536`、`88073736`、`0b01089c`）
- **导出修复** — 修复导出音频/视频未使用编辑器最新 tempo 的问题，修复导出 MIDI 时 LLVM 向量化崩溃（`92daf259`、`cb584f2b`）

### 播放、滚动与视口

- **播放修复** — 修复空白工程中 tempo 编辑设置被静默丢弃，统一当前轨与其他轨的文档流式读取（`13cacd10`、`47658470`）
- **滚动体验** — 统一滚轮/触控板水平滚动方向为自然滚动，修复触控板斜向滚动退化为单轴，添加标尺和键盘区域的滚轮缩放支持（`e80aa929`、`6cd76217`、`165a4543`）
- **视口修复** — 修正垂直缩放以标尺区分隔的视口高度计算，缩放时终止遗留平滑滚动动画（`cc841dd9`、`87e6f0da`）
- **精度一致性** — 修复 PPQ 变更后精度显示与吸附行为不一致的问题（`55303713`）

### UI 与交互

- **音轨拖拽排序** — 支持音轨拖拽排序与视觉-文档索引映射，修复音轨列表响应范围外鼠标事件、新建音轨后无法放置音符等问题，移除 Delete 快捷键删除功能（`cfc267be`、`7afe9455`、`5d700784`、`5eac67d8`）
- **右侧栏修复** — 修复右侧栏拖拽宽度回撤并卡死、非钢琴卷帘视图错误渲染、拖拽宽度回撤的问题，更新文字颜色以适配明暗主题（`8e2e1a86`、`1a184f6a`、`1279dafd`）
- **对话框修复** — 修复恢复对话框按钮文字居中、永久删除音轨缓存后对话框不关闭、设置窗口尺寸等问题（`e253272e`、`6693b231`、`8d9aca87`）
- **监控设置** — 新增底边栏监控数据刷新间隔设置（`e29e68b6`）
- **其他修复** — 修复 Sidebar 消息误报未处理警告、精度标签在暗色主题下不可见（`a0cafec5`、`e97e8f1e`）

### 平台适配与依赖

- **macOS 菜单** — 新增禁用态菜单项支持并修正菜单初始化逻辑（`4ef7ae0f`）
- **应用图标** — 添加应用图标资源（`c0412d67`、`60cbecad`）
- **archive-reader** — 移除 ZPAQ 支持并添加 advapi32 链接（`61c92e65`）
- **依赖调整** — 切换 xsynth 系列依赖到 GitHub fork，移除 midi-io 的 new-audio-backend 特性开关并精简依赖（`9bb0c5f8`、`a964843d`）

### 内存监控

- **看门狗范围** — 看门狗仅在 MIDI 加载期间监控内存（`54ddaf6d`）
- **macOS 适配** — 修复 macOS RSS 获取因结构体不完整失败的问题，在 macOS 跳过 RSS 相关测试（`edfde4b1`、`c254ff96`）

### 重构

- **音符存储单一权威源** — 移除 NoteStore 冗余镜像，统一使用 notes 单一权威源，重构音符存储（`441a1ca5`、`cde301bd`）
- **midi-model API** — 添加音符增删改 API 及单元测试（`edd84bc4`）
- **移除事件浏览器** — 移除事件浏览器及相关功能（`cb0088bd`）

### 测试与 CI

- **测试数据自举** — midi-loader 测试数据自举，缺失本地文件时回退生成数据（`40994e21`）
- **协作测试** — 改进加入房间失败时的错误信息（`fa7c6453`）
- **unwrap 清理** — 清理全部测试代码 unwrap，替换为带中文上下文的 expect（`0c1370a0`）
- **CI 增强** — 增强跨编译与缓存优化工作流（`8ecbe164`、`e2fd5ebe`）

### Chore

- **版本号更新** — 更新版本号为 0.2.0（`43e99fac`）

---

## [0.1.0] - 2026-06-09 — Stable

### Added

- **演奏指示线拖拽交互** — 支持点击画布拖动演奏指示线位置（`a34e068`）
- **固定演奏指示线拖拽** — 添加固定演奏指示线的拖拽交互功能（`6df8db8`）
- **菜单和窗口事件可读名称** — 为所有菜单和窗口事件添加可读显示名称，便于调试（`f2851e2`）

### Fixed

- **力度面板坐标映射修复** — 修复力度面板坐标映射和绘制偏移问题（`8d6908d`）
- **洋葱皮渲染竞态修复** — 修复洋葱皮渲染竞态条件并优化渲染触发逻辑（`24ff5d3`）
- **变速/移调音符校验** — 为变速和移调功能添加选中音符校验，防止空选区操作（`832e57e`）
- **滚动条缩放震荡修复** — 修复拖拽滚动条时的缩放震荡问题（`d741b57`）
- **底部面板与画布间隙修复** — 修复底部面板与画布之间的渲染间隙问题（`bdf0b05`）
- **GitHub Actions 修复** — 修复 CI 构建流程中的问题（`1d41e8b`、`7df4eab`）

### Changed

- **统一走带与卷帘滚动/指示线样式** — 统一工程走带与钢琴卷帘的自动滚动和播放指示线样式（`285dd08`）
- **Velocity 面板背景渲染统一** — 统一 velocity 面板背景渲染逻辑（`eb2e765`）
- **Tempo 模式绘制整合** — 将 Tempo 模式绘制逻辑整合到通用绘制函数（`eb8c932`）
- **网格线渲染迁移** — 将 velocity 面板网格线渲染从 WGPU 移到 Canvas 直接绘制（`c7a08d6`）
- **工具栏高度常量提取** — 提取 velocity 编辑器工具栏高度为常量并修复 resize 逻辑（`54ecdcc`）
- **Tempo 模式注释清理** — 移除 tempo 模式遗留注释并实现背景横线绘制（`0a2f390`）

### Removed

- **滚动条 `lock_zoom_out` 字段** — 移除拖拽边缘状态的未使用字段（`04c576d`）
- **xsynth 多线程配置** — 移除设置面板中 xsynth 多线程相关配置项（`15260bb`）

### Refactored

- **macOS 原生菜单重构** — 重构原生菜单构建逻辑，修复子菜单支持问题（`801973e`）
- **内存监控 macOS 适配** — 适配 macOS 平台，禁用默认内存监控和看门狗（`b0869c9`）

---

## [0.1.0-beta1] - 2026-06-08

### 工程与构建

- 切换项目依赖源至 `PenguinBMDevs` 组织仓库
- 更新 xsynth 依赖到新 commit 版本
- 更新依赖并简化跨编译脚本
- 添加跨平台构建和发布脚本

### 编辑器核心

- **速度编辑器** — 实现速度编辑器核心功能，BPM 上限扩展至 10000，支持标尺绘制
- **CC 控制器编辑器** — 实现 CC 控制器柱状条渲染与编辑功能，整合到力度面板
- **弯音编辑** — 添加弯音编辑支持，重构 CC 编辑界面
- **音符分割与合并** — 实现音符分割与合并功能，添加工具栏按钮
- **移调功能** — 实现移调功能并添加工具栏按钮
- **音符翻转** — 添加音符水平/垂直翻转功能
- **音符变速** — 添加基础音符变速功能及对话框
- **音轨混音器** — 实现音轨混音器功能
- **速度包络视图** — 实现速度包络视图
- **音轨总览模式** — 实现音轨总览视图与 Pattern 编辑功能，含缩略钢琴卷帘渲染

### 编曲视图（Arrangement）

- 实现工程走带 GPU 渲染器（`gfx` 模块）
- 添加工程走带滚动缩放与演奏指示线
- 添加工程走带垂直缩放功能
- 重构工程走带渲染逻辑，修复同步问题
- 重构音轨列表为独立 Canvas 组件，修复滚动计算
- 实现音轨总览模式下的缩略钢琴卷帘渲染

### UI 与交互

- **设置对话框** — 实现完整的设置对话框功能，含设置变更检测与同步
- **工程设置弹窗** — 新增工程设置弹窗，支持项目名称、BPM、版权信息编辑
- **音频导出** — 添加音频导出功能及导出对话框
- **工程导出与加载** — 实现完整的工程导出与加载系统
- **高对比度主题** — 添加高对比度主题支持
- **Tooltip 组件** — 统一封装 tooltip 组件并全局复用
- **框选动画** — 为框选框添加弹簧弹性动画效果，支持两种显示模式
- **平滑滚动** — 实现平滑滚动动画功能，支持 Shift+滚轮横向滚动
- **循环播放** — 新增循环播放回绕功能及循环区域 UI
- **音符量化** — 添加音符量化功能
- **MIDI 录制** — 新增 MIDI 录制功能
- **力度曲线绘制** — 添加力度曲线绘制功能（画笔模式）
- **性能监控面板** — 添加性能监控面板，支持浮动显示
- **编辑器/瀑布流模式切换** — 新增编辑器与瀑布流模式切换功能
- **自适应网格线** — 实现自适应网格线，新增半拍/网格线颜色常量
- **刻度标签** — 为力度/CC/弯音编辑器添加刻度标签绘制
- **MIDI 可视化颜色映射** — 为 MIDI 可视化添加基于参数的颜色映射
- **主题色统一** — 移除硬编码颜色值，统一使用主题色渲染

### 渲染与性能

- 全面迁移到 WGPU 渲染管线，移除旧的 Canvas 渲染实现
- 新增 `NoteInfo` 预解析缓存，优化音符渲染性能
- 优化渲染线程命令处理与侧边栏逻辑
- 减少不必要的渲染和事件处理，提升 UI 性能
- 优化 UI 重绘逻辑，区分重建和重绘时机

### 修复

- 修复编曲模式缩放和渲染异常
- 修复非 macOS 平台画布偏移对齐问题
- 修复视口高度计算和轨道列表渲染问题
- 修复主题缓存未及时更新导致的对话框主题不一致问题
- 修复 dialog 对话框标题栏信息刷新 BUG
- 修复工程设置对话框确认/取消不关闭窗口及标题不更新的问题
- 修复循环区域拖拽、量化工具交互及 MIDI 录制回归缺陷
- 修复先开循环再创建播放管理器不同步的问题
- 修复拖动选区时未对齐时间轴刻度的问题
- 修复 ffmpeg 缓冲区过大导致的 OOM 问题
- 修复洋葱皮渲染视口变化时缓存未失效的问题
- 修复零实例绘制时的幽灵音符问题
- 修复 tick 0 处 tempo 被默认 120 BPM 覆盖的问题
- 修复配置文件解析失败时使用默认配置

### 重构

- 重构文件保存与导出逻辑，统一处理流程
- 重构音频导出模块，拆分代码并优化内存占用
- 重构编辑器状态管理模块结构，引入缓存失效标志位
- 全面重构 Editor 状态管理（绞杀者模式迁移至 EditorState）
- 重构框选逻辑使用世界坐标替代屏幕坐标
- 重构洋葱皮渲染架构，分离主音符与洋葱皮计算
- 重构音符渲染工作循环，优化滚动性能
- 重构分离渲染线程架构
- 全面清理重复代码和死代码，消除跨 Crate 类型冗余
- 重构 DMS 复合节点创建逻辑，将解析逻辑移至 `DmsReader`
- 重构 MIDI 输出接口实现
- `save_storage` 神函数拆分 + `file_menu` 内存日志提取
- `scan_track_names` 神函数拆分 + export 转换器测试
- Host 上帝结构体拆分——`RenderContext` + `WindowContext`
- `memory_monitor` 模块拆分 + `GridCameraUniform` Builder 模式

### 测试

- 添加 DMS 单元测试/集成测试
- 为视图布局模块添加单元测试
- 修复外网依赖测试标记 `ignore`

### 杂项

- 全项目零警告冲刺——清理 19 个编译警告
- 移除废弃的内存管理模块及相关代码
- 清理旧渲染架构并移除未使用代码
- 使用 `parking_lot` 替换 `std::sync::Mutex` 提升性能

---

## [0.1.0-preview1] - 2026-03-07

### 初始预览版

首个公开预览版本，包含基础 DAW 编辑器框架。

#### 核心功能

- **钢琴卷帘编辑器** — 基础音符编辑、网格对齐、缩放滚动
- **WGPU 渲染引擎** — 分离渲染线程架构，GPU 音符渲染与裁剪
- **MIDI 支持** — 文件导入、播放（XSynth 合成器）、KDMAPI/系统 MIDI 输出
- **DMS 工程格式** — 工程文件读写、流式加载、进度显示
- **音频导出** — 基础导出功能
- **主题系统** — 亮/暗主题切换
- **自定义标题栏** — 工具栏、菜单、路由导航
- **洋葱皮渲染** — 多音轨参考显示
- **音轨列表** — 虚拟滚动、侧边栏
- **多人协作** — WebSocket 联机编辑（实验性）
- **跨平台** — Windows / macOS / Linux 支持

#### 技术特性

- 自定义 `winit` + `wgpu` 运行时（脱离 `iced::application`）
- 分离渲染线程 + 双缓冲音符实例数据
- 分层缓存系统支持黑乐谱大文件
- 流式播放引擎支持多轨读取
- 内存监控与优化
- HiDPI 支持
- 触摸事件支持
- SVG/PNG 图标系统

---

## [0.1.1] - 2026-07-13 — Penguin Studio Stable

### 视频导出系统

- **全新视频导出功能** — 基于 FFmpeg 的完整视频导出管线，支持高精度贴图渲染模式导出视频（`e06231b`、`d340705`）
- **视频导出对话框** — 新增视频导出对话框，支持预览帧、参数配置与取消操作（`61ff7b5`、`61cf850`、`d258564`）
- **键盘贴图与标尺合成** — 视频导出时合成键盘贴图与标尺小节号（`d87e624`、`a14547c`）
- **四重缓冲流水线** — 改用四重缓冲流水线大幅提升导出性能（`2399f21`）
- **导出健全性** — 取消导出时正确收尾编码器，传递导出耗时与帧率信息（`4677f3d`、`cabf98a`、`217a399`）
- **状态同步与修复** — 同步视频导出状态至对话框窗口，修复死锁、预览空白、键盘渲染对齐、音符渲染残留等问题（`73e1a85`、`fc9e332`、`26d13dd`、`75422bc`、`b0a6c51`、`f1a6b25`、`ea107cd`、`44b3c65`、`51241ef`）

### 实时合成引擎

- **实时合成器模块** — 新增 `realtime` crate，包含实时合成器核心与 RealtimeEventEngine（`222fa96`、`706d837`）
- **线程安全** — 添加渲染线程运行标志控制生命周期，使用无界通道与非阻塞发送防止事件丢弃（`897eeee`、`ad83b8b`、`009f879`）
- **渲染优化** — 避免缓冲区零初始化并启用多线程，修复音频渲染丢帧与静音问题（`896390b`、`d29de82`、`a825d4e`）

### 音频导出增强

- **流水线架构重构** — 重构音频渲染为流水线架构，提升可维护性（`5d8eeb4`）
- **后台渲染** — 在后台线程执行音频渲染，避免阻塞 UI（`f36f1e2`）
- **性能优化** — 使用 mmap 与流式迭代优化内存占用，加速离线渲染（`f53f244`、`5f90526`）
- **格式与过滤** — 添加音频导出格式选择与事件过滤选项（`b4b7a96`）

### 播放引擎优化

- **事件处理修复** — 重置 MIDI 事件游标并更新播放音符数据，仅处理非当前音轨的控制事件，避免重复发射 CC（`ea48f20`、`1eb557d`）
- **调试能力** — 添加 CC 和 PitchBend 事件调试日志，增强播放更新可追踪性（`bd4b031`）
- **性能优化** — 优化音频线程定时精度与内存分配，优化 MIDI 事件处理与渲染帧事件预算（`3a07bce`、`8128f63`）

### 编辑器与 UI

- **琴键播放指示** — 播放期间琴键叠加洋葱皮颜色，添加播放键盘颜色指示开关（`6c79755`、`46a98e9`）
- **琴键性能优化** — 用固定数组替代 HashMap，分离洋葱皮绘制与懒构建空间索引（`c69d994`、`f787508`）
- **力度面板** — 添加力度面板曲线/柱状图样式切换功能（`2a49a0a`）
- **曲线编辑** — 添加曲线编辑工具的使用限制及自动化拖拽反馈绘制（`2e7fa33`）
- **音轨交互** — 添加音轨后自动切换到新音轨，支持音轨行为配置（`f05a10e`、`1efacc1`）

### 配置与存储

- **格式迁移** — 配置文件格式从 TOML 迁移至 JSON（`356d6f6`）
- **目录迁移** — 迁移旧版配置目录至新路径（`0b6e854`）
- **会话计时** — 添加会话计时跟踪器，记录工程创建时间和累计编辑时间（`a74a4c9`）
- **自动滚动修复** — 修复自动滚动配置未正确保存与应用的问题（`174e83c`）

### 重构与架构

- **对话框 crate** — 抽取对话框管理至独立 `dialog` crate，消除 500+ 行神类（`50b9f7c`）
- **渲染拆分** — 拆除 window.rs 中的渲染部分，分离关注点（`2ccb2c2`）
- **侧边栏简化** — 移除添加音轨菜单展开状态，简化侧边栏面板逻辑（`e8d5dce`）
- **轻量窗口渲染** — 优化轻量窗口渲染与启动流程（`07df120`）
- **录制回滚** — 录制失败时回滚工具栏状态并同步设备列表（`3b0df1f`）

### 高精度贴图优化

- **脏区覆层系统** — 移除大文件加载确认并添加高精度脏区覆层系统（`071d8ed`）
- **修复与清理** — 修复脏覆层被误清除与原始长度跟踪问题，移除冗余诊断日志（`ad96d4e`、`0353619`、`f860aa0`、`f8b577f`）
- **空间索引优化** — 移除洋葱皮空间索引缓存改用二分查找（`78628fa`）

### Fixed

- **ICO 编译修复** — 修复 ICO 资源编译错误，添加可选清单处理（`aef4829`）
- **README 更新** — 更新 README 描述并修复链接（`38d2886`）

### Chore

- **组织标识符** — 更新项目组织标识符为 PenguinBMDevs（`b8da504`）
- **Lint 配置统一** — 统一工作区 lint 配置并移除 unwrap 以及 mod.rs（`3103389`）
- **移除钢琴仿真键盘设置** — 移除未使用的钢琴仿真键盘设置项（`0440ff9`）
- **清理工作区** — 清理工作区临时文件（`8e86010`）

---

## [0.1.1-beta1] - 2026-07-08 — Penguin Studio BETA1

### 协作功能初始实现

- **多人实时协作编辑** — 实现多用户协作音轨同步与删除同步（`0eb7833`）
- **协作覆盖层系统** — 新增协作覆盖层系统与编辑音轨跟踪，支持多用户同时编辑可视化（`3e64002`）
- **tokio runtime 嵌套修复** — 修复协作模块 tokio runtime 嵌套 panic 问题（`27016d8`）

### 音频导出系统重构与增强

- **GPU 加速音频导出** — 新增基于 WGPU Compute Shader 的 GPU 加速 MIDI→WAV 音频导出，支持从内存渲染（`5d94097`、`8d95b2c`）
- **双缓冲非阻塞渲染** — 重构 GPU 音频渲染流水线，实现双缓冲非阻塞渲染模式（`bf8862d`）
- **GPU MIDI 渲染性能优化** — 预提取全量 MIDI 事件，大幅提升渲染吞吐（`5eee0ba`）
- **导出进度条面板** — 新增内嵌进度条面板，按时间刷新导出进度及统计展示（`e44f4b9`、`244f111`）
- **音频导出流程重构** — 重构音频导出流式渲染逻辑，修复漏音问题，移除全量事件缓存（`6442c39`、`1e8550e`）
- **导出面板完成** — 完善音频导出面板功能，添加音频/视频导出侧边栏入口（`353dff1`、`4ed5798`）
- **移除旧音频后端** — 移除旧音频导出后端相关代码（`a4bff7b`）
- **SF2 采样修复** — 修复 SF2 采样播放偏移与循环逻辑问题（`777e762`）

### 侧边栏重构与新功能

- **侧边栏分组系统** — 重构侧边栏为分组系统，统一面板状态管理（`6e03d24`）
- **工程走带升级** — 将工程走带升级为独立分组，恢复遗漏的走带按钮（`6d06085`、`a72ff14`）
- **侧边栏互斥逻辑优化** — 优化侧边栏路由互斥逻辑与多面板文案（`c1e2ecb`）
- **瀑布流模式切换** — 添加瀑布流模式切换和音频导出侧边栏按钮（`d955b71`）
- **钢琴卷帘分组修复** — 修复钢琴卷帘分组无法再次打开的 BUG（`dcf78f6`）
- **缓存系统** — 添加缓存清理与读写操作功能，移除缓存目录忽略规则（`7049037`、`65305fd`）

### 代码质量与架构重构

- **unwrap → expect 全量替换** — 替换代码中所有 `unwrap()` 为 `expect` 并添加描述性错误提示（`058ad95`）
- **编辑器状态模块化** — 将编辑器状态拆分到独立子模块，提取编辑器逻辑到独立模块，统一视口操作（`ffde34a`、`f7e2ec7`）
- **跨 crate 类型依赖重构** — 将共享类型移入 `core` crate，减小跨模块耦合（`635c982`）
- **渲染架构简化** — 重构渲染参数与渲染器管理，简化代码结构（`4884fa7`）
- **常量统一与代码清理** — 统一常量使用，重构项目代码结构，清理未使用的导入/模块/废弃代码（`6dfa51b`、`95e7b15`、`ded2178`、`2edd72f`）
- **事件构造函数统一** — 整理窗口事件构造函数，统一使用直接构造方式（`47f7d80`）
- **图标反色抽象** — 抽象图标反色判断逻辑，新增专用测试（`28feae0`）
- **Velocity 编辑器坐标重构** — 重构 tempo 坐标映射逻辑，从对数改为线性缩放（`69ee6f2`、`197cdf4`）

### 高精度洋葱皮贴图优化

- **贴图系统重构** — 重构高精度贴图系统，优化内存与渲染逻辑（`3c3c559`）
- **脏区域即时覆层** — 新增高精度贴图脏区域即时覆层刷新功能（`922f918`）
- **脏区域覆层逻辑优化** — 优化脏区域覆层渲染逻辑并补充测试（`464c847`）
- **贴图与走带性能优化** — 优化高精度洋葱皮贴图与工程走带视图性能（`6d8ae0b`）

### MIDI 与渲染优化

- **XSynth 并发限制** — 新增 XSynth 全局并发 voice 限制与力度过滤重构（`1d1937b`）
- **音符渲染性能** — 优化音符渲染性能，增加视口裁剪与缓存机制（`b7bd679`）
- **KDMAPI 重构** — 重构并完善 KDMAPI 后端实现（`5eb1586`）
- **代码格式优化** — 优化代码格式，简化模块导入和错误处理（`ecc920e`）

### Fixed

- **音符事件通道死信修复** — 修复分离渲染线程下音符事件通道死信问题（`3cd5dec`）
- **GPU 音频渲染 Bug 修复** — 修复 GPU 音频渲染的多个 bug 并添加诊断日志（`ad9472e`）

---

## [0.1.1-preview1] - 2026-07-04

### 工程与架构重构

- **项目结构重整** — 拆分 monolith 为独立 crates（`midi-loader`、`midi-io`、`playback`、`event`、`message`、`memory-monitor`、`onion-skin`），项目 Crate 数从 8 增长至 16（`53bcc68`）
- **编辑器核心抽离** — 将 `editor_state`、`spatial_index`、`history`、`note`、`transform`、`view_state` 等核心逻辑迁移至 `lumino-core` crate（`45250a1`、`618b810`、`38e4fcd`、`48bdc08`）
- **渲染线程迁移** — 将 `wgpu_render_thread` 整体迁移至 `lumino-gfx` crate，消除 UI crate 对 GPU 的直接依赖（`786d5b9`、`fb237b1`）
- **事件系统独立** — 将 `core::event` 模块抽取为独立 `lumino-event` crate，统一窗口/菜单/对话框事件类型（`d87f882`、`1bb07b8`、`ca82f18`）
- **消息类型集中** — 创建 `lumino-message` crate，统一跨模块消息传递（`78fedc1`）
- **导出模块迁移** — 将 `core::project` 相关逻辑迁移至 `lumino-export` crate，统一 LMPJ/MIDI 导入导出（`d60c22c`、`2bbdae4`、`79e0247`）
- **代码清理** — 清理废弃代码、统一魔法数值、优化错误处理（`9a6037b`、`865a5ad`、`9fb20cc`）

### Added

- **多语言国际化系统（i18n）** — 完整 i18n 框架，覆盖编辑器、设置面板、工具栏、菜单、对话框等全界面（`c9d3174`、`5705496`、`cf50632`、`31e1b60`、`b272615`）
- **高精度洋葱皮贴图系统** — 全新 `onion-skin-hires` crate，基于 WGPU Compute Shader 实现 GPU 端高精度贴图生成（`fe70377`、`bf1c05a`、`3698a09`、`c83f346`、`b449eb2`、`9a6e156`、`ef48708`）
  - 高精度贴图首帧生成/缓存/调度整套管线（`72461df`）
  - 流式后台静默重生，按音轨组合并处理（`81c41de`、`84c272e`、`2d2b915`）
  - 脏区域临时覆层，编辑可见性与冷静期重生成（`4fa4ed6`）
  - 设置面板与控制开关（`89c98d4`、`c4639c9`）
  - GPU compute 剔除 + 间接绘制架构（`207dda2`）
  - 并行上传 + 音符过滤优化（`e33f48a`）
- **自动化面板侧边栏** — 新增自动化控制面板（`d9188fb`）
- **钢琴卷帘侧边栏切换** — 支持侧边栏路由切换（`21dc4d3`）
- **实时音频引擎与 WAV 导出** — 基于 ring buffer + 同步渲染引擎的全新音频架构，支持实时播放与 WAV 文件导出（`6bf38a7`、`5f42ed1`）
- **MIDI 输入后端选择** — 支持 KDMAPI / 系统 MIDI 后端动态切换（`8b28b29`）
- **NoteEvent 单条音符类型** — 替代 NoteOn/NoteOff 事件对，简化音符存储与处理（`e4d1af8`）
- **音频模型加载拆分** — 实时播放与离线导出各用独立模型加载路径（`dc74e3f`）
- **MIDI 录制功能增强** — 全音符关闭与重置控制器事件处理（`c039392`）
- **滚动速度追踪与 overscan 补偿** — note-worker 新增智能滚动预测（`8deb506`）
- **macOS 平台菜单初始化语言支持**（`d445547`）
- **NW 性能基准测试**（`00c5d1f`）
- **安全策略与行为准则** — 新增 `SECURITY.md` 和 `CODE_OF_CONDUCT.md`（`b98a67b`、`84df3d2`）
- **修复后的应用图标** — 重新生成 `lumino.ico`，修复 RC2176 old DIB 错误（`a9e5c54`、`4ef1cfc`、`10dc57f`）
- **高精度贴图性能测试** — 贴图生成性能测试与基准（`60b94b5`）
- **高精度贴图概览模式** — P1.5 集成概览贴图到主应用（`83d2769`）

### Fixed

- **音频 Critical 级别 Bug 修复** — 修复音频模块导致程序无响应的关键问题（`ae34cac`）
- **播放引擎响应性修复** — 修复播放引擎响应迟钝与 MIDI 重初始化后无声问题（`ca87018`）
- **洋葱皮贴图空白修复** — 修复 tick 单位构造器未填充 `start_ms`/`end_ms` 导致的空白贴图（`e555bae`）
- **WGPU 验证错误修复** — 修复 pipeline color format 不匹配与 uniform 可见性导致的验证层错误（`088f69a`、`1d8796c`）
- **洋葱皮运行时缺陷修复** — 修复洋葱皮运行时缺陷并改用 Nearest 采样提升清晰度（`f9839ef`）
- **2GB 显存截断修复** — 洋葱皮按时间分块渲染，突破单贴图 2GB 显存上限（`a8c9ce5`）
- **洋葱皮音轨消失修复** — 修复部分音轨整体消失的渲染 Bug（`b1adbad`）
- **虚拟滚动 Bug 修复** — 修复卷帘模式音轨列表只显示前 40 条的问题（`fba34ee`）
- **滚动越界修复** — 修复滚动越界问题并调整滚动方向逻辑（`c7d2add`）
- **琴键和网格绘制溢出修复** — 修复绘制溢出与缩放留白问题（`046e6f6`）
- **棋盘格与留白修复** — 修复音符区域可见范围计算与游标跳过可见音符的问题（`b7695af`、`ad3ec45`）
- **Shift+滚轮水平滚动符号修复** — 修复水平滚动方向与直觉相反的问题（`22cde1b`）
- **对话框闪跳修复** — 修复对话框闪跳和变速按钮禁用问题（`a4df1ce`）

### Changed

- **许可证变更** — 项目许可证从专有变更为 AGPL-3.0-or-later（`a9e3f3f`）
- **版本升级** — 项目版本从 0.1.0 升级至 0.1.1-dev（`533cc68`）
- **ONNX 推理移至后台线程** — 避免阻塞渲染线程（`5f42ed1`）
- **MIDI 加载器完全重写** — 使用 midly 流式 API 替代全局中间态，支持 RIFF 封装 MIDI 格式（`eed8e8e`、`02f11ec`、`59940ee`、`6f42155`）
- **低精度洋葱皮渲染移除** — 全面移除旧版 CPU 洋葱皮渲染代码，迁移至独立 `onion-skin` crate（`57b41b5`、`8a62463`、`da84265`）
- **LMPJ 加载导出重构** — 移除冗余原始 MIDI 字节缓存（`08617ef`）
- **midly-fork 依赖更新** — 更新到支持并行流式 API 的新版本（`13695dc`）
- **README 与仓库 URL 更新** — 文档信息同步（`a29a97c`）
- **Cargo 依赖更新** — 全面更新依赖版本（隐于 `Cargo.lock`）

### Refactored

- **高精度洋葱皮渲染管线重构** — 替换分桶缓存为扁平音符列表，GPU 全量扫描替代 CPU 二分裁剪，移除轨道掩码和颜色 uniform（`4464ddb`、`7e34e8b`、`d63a7e5`、`818fa72`）
- **洋葱皮渲染分桶逻辑重构** — 实现增量缓存与游标复用，按 key 分桶缓存增量更新（`2144ec9`、`57c704a`）
- **洋葱皮渲染线程重构** — 移除独立工作线程，采集逻辑移至渲染线程，修复剔除计数器竞态（`47b9a57`、`fa0e4ac`）
- **高精度贴图模块重构** — 优化性能与体验（`9f3786b`）
- **渲染线程流程重构** — 将高精度洋葱皮渲染线程流程整体重写（`8558b60`）
- **音频播放重构** — 重构其他音轨事件处理逻辑，改用流式按需生成（`59baa89`）
- **编辑器状态重构完成** — 空间索引和洋葱皮状态提取为独立结构体（`76e1ca2`、`2c951fe`）
- **MIDI 相关模块拆分** — 独立测试文件与 spatial 路径修复（`e33ef8f`、`968da93`）
- **事件处理器拆分** — 拆分 `root/handlers.rs` 神处理器，按功能域独立（`78fedc1`、`2418abe`）
- **文件菜单重构** — `file.rs` 神函数拆分为 `export`/`load`/`save`/`helpers` 子模块（`9fb20cc`）
- **velocity 编辑器重构** — 拆分 velocity 面板绘制/状态逻辑（`79e0247`）
- **loop_range 模块重构** — 拆分 loop_range 为独立模块并补充测试（`ca82f18`）
- **音符分割/合并/翻转/移调/变速重构** — 通用函数提取，消除重复逻辑（`79e0247`、`76e1ca2`）
- **MIDI handler/parser 重构** — 播放引擎与 MIDI 处理逻辑解耦（`968da93`、`d60c22c`）

### Perf

- **洋葱皮回归 GPU 驱动架构** — 解决滚动卡顿，大幅提升渲染帧率（`58efa95`）
- **洋葱渲染器上传与剔除优化** — 减少 GPU 数据量（`501d49f`）
- **移除 CPU 端排序与过滤** — 洋葱皮渲染性能进一步优化（`84058c1`）
- **贴图生成速度优化** — 引入 scale 预乘，批量化 `copy_from_slice` 并合并 u32 节省内存（`60b94b5`）
- **音符数据零拷贝优化** — 调整模块顺序实现零拷贝（`a84f057`）

### Removed

- **废弃的 `audio` crate 代码** — 清理旧音频模块（`7c9da38`）
- **MIDI 输入后端相关死代码** — 简化音频播放链路（`154609b`）
- **旧版洋葱皮渲染器** — `onion_renderer` 整模块移除（`57b41b5`）
- **旧 `wgpu_render_thread`** — 迁移至 gfx crate 后移除（`786d5b9`）
- **`core::event`** — 迁移至独立 `lumino-event` crate 后移除（`78fedc1`）
- **`core::project`** — 迁移至 `lumino-export` 后移除（`d60c22c`）
- **`core::midi`** — 迁移至 `midi-loader` 后移除（`968da93`）
- **`ui::playback`** — 迁移至 `lumino-playback` crate 后移除（`48bdc08`）
- **`middleware`** — 废弃中间件模块（`9a6037b`）
- **`onion_bg_pool`** — 废弃背景池（`9a6037b`）
- **`onion_skin_editor`/`onion_skin_ops`** — 废弃低精度编辑相关（`9a6037b`）
- **旧版 `loop_range` 实现** — 替换为独立模块（`ca82f18`）
- **旧版 `note_flip`/`note_ops`/`note_split_glue`/`note_transpose`** — 统一迁移至核心模块（`76e1ca2`）
- **旧资源文件** — 清理旧版 logo 资源（`dbaff15`）
- **废弃测试资源** — `tests/common/mod.rs` → `tests/common.rs` 扁平化（`2418abe`）
- **`onion_skin_bench` 测试** — 低精度性能测试不再需要（`57b41b5`）

### CI/CD

- **新增 Windows CI** — `fast-check.yml` + `fast-release.yml` + `ci.yml` 三套并行工作流（`437e9a9`、`05c9832`、`eec463c`）
- **fast-release 纠正** — 修复错误行为并添加 ALSA 开发库安装（`31a2dd2`、`1a258ad`、`9ab5c4a`、`25be638`）
- **简化 Workflow** — 移除 clippy 步骤以加速构建（`a095f0e`）

### Docs

- **黑乐谱优化文档** — 新增 `docs/black_midi_optimization.md`，详细记录黑乐谱大文件加载与渲染优化方案
- **技术债务备忘** — 新增 `docs/technical-debt.md`，记录已知技术债务与待重构项）

---

## [Unreleased]

### Planned

- 性能进一步优化
- 更多 MIDI CC 支持
- 插件系统
# CIFIX-001 · ubuntu lint 与 BOM 守卫红灯修复记录

- **日期**：2026-10-18
- **触发**：`dev` 推送（S0 形状底座合并，`b530ee6f`）后 CI Check 两处红
- **结论**：两条红灯**都是存量问题**，与 S0 无关；根因分别是「平台条件编译把 API 误用藏起来」与「编辑器保存行为把 BOM 写回 5 个 `.rs`」
- **验证口径**：本机 WSL 的 Ubuntu 26.04 上跑 CI 同款命令（不再依赖"推上去等 CI 反馈"）

---

## 1. 谁把它弄红的（先定责，再修）

| 红灯 | 引入提交 | 时间 | 归属 | 潜伏期 |
|---|---|---|---|---|
| ubuntu lint `E0599 with_user_event` | `a047e613` REND-019 PR-C（#152） | 2026-10-16 23:12 | 既有 dev 提交，**非本次 S0** | ~2 天 |
| BOM 守卫 5 个 `.rs` | `6dc5f82b` / `dc658893`（REF-001 阶段 G 合并） | 2026-10-17 | 既有 dev 提交，**非本次 S0** | ~1 天 |

判定依据（可复核）：

```bash
git merge-base --is-ancestor a047e613 ebc7c568   # 旧 dev 头已含坏代码 → exit 0
git rev-parse ebc7c568:src/runner/window_manager.rs   # 09e127de…（含 BOM）
git rev-parse b530ee6f:src/runner/window_manager.rs   # 09e127de…（同 blob）
git show ebc7c568:.github/workflows/ci.yml | grep bom-guard   # 守卫 job 当时已在
```

即：**旧 dev 头本身就是红的**，S0 的合并只是把这两条存量红灯"继承"过来并显性化。

---

## 2. 红灯一：`with_user_event` 找不到（ubuntu lint）

### 2.1 现象

```
error[E0599]: no associated function or constant named `with_user_event` found for struct `EventLoopBuilder<T>`
  --> src/main.rs:87:48 / 91:66 / 97:48
error: could not compile `lumino-rs` (bin "lumino-rs") due to 3 previous errors
```

### 2.2 五问根因（RCA）

| # | 问题 | 答案 |
|---|---|---|
| 1 | 为什么编译不过？ | `with_user_event()` 不存在于 `EventLoopBuilder<T>` |
| 2 | 为什么不存在？ | winit 0.30 里它属于 `EventLoop<T>`：`impl<T> EventLoopBuilder<T>` 只有 `build()`，`impl<T> EventLoop<T>` 才有 `with_user_event() -> EventLoopBuilder<T>` |
| 3 | 为什么本机（Windows）没发现？ | 三处调用全在 `#[cfg(target_os = "linux")]` 的 `create_event_loop()` 内 → **cfg 剥离**，Windows 上该函数根本不参与类型检查 |
| 4 | 为什么能进 dev？ | 作者在 Windows 开发；唯一会编译这段代码的 ubuntu job 当时已经是红的（BOM 那一条）→ 新红旧红混在一起，没人能分辨 |
| 5 | 为什么"红着还能推"？ | 两个门禁都只在 CI 跑，本地没有对应预检；且推送者本机无 `gh`，看不到 CI 结果 |

**病根不是拼错 API，是"cfg 剥离盲区 + 红着推"**。只把 API 改对，下一次换个平台专用分支还会再炸一遍。

### 2.3 源码事实（winit 0.30.13，本地 registry 直读）

| 位置 | 内容 |
|---|---|
| `src/event_loop.rs:71` | `impl EventLoopBuilder<()>` → 只有 `new()`（已 deprecated） |
| `src/event_loop.rs:80` | `impl<T> EventLoopBuilder<T>` → 只有 `build(&mut self)`（:115） |
| `src/event_loop.rs:215` | `impl<T> EventLoop<T>` → `pub fn with_user_event() -> EventLoopBuilder<T>` |

### 2.4 修法（不只是改对，还要让同类问题在本机可见）

1. **API 改正**：三处调用改走 `EventLoop::<runner::UserEvent>::with_user_event()`；
2. **抽成不带 cfg 的 `user_event_builder()`**：把这条 API 调用从 Linux 专用分支里提出来，
   变成任意平台都会编译的共用函数 —— 本机 `cargo check` 从此能拦住同类误用；
3. **补编译期契约测试** `tests::user_event_builder_matches_winit_api`：谁改回错误写法，
   三个平台的门禁一起红，而不是只在 ubuntu 红。

> 取舍说明：没有把 Wayland/X11 分支整个搬出 cfg（那是平台分支的本职），只把**跨平台可编译的那一行 API** 提出来——改动面 1 个函数 + 3 处调用 + 1 个测试。

---

## 3. 红灯二：5 个 `.rs` 带 UTF-8 BOM

### 3.1 现象

```
::error::检测到 UTF-8 BOM（EF BB BF）：5 个 .rs 文件命中
  BOM: crates/editor/dialog/src/window/initialize.rs
  BOM: src/runner/cloud_progress.rs
  BOM: src/runner/device_warning.rs
  BOM: src/runner/progress_manager.rs
  BOM: src/runner/window_manager.rs
```

### 3.2 五问根因

| # | 问题 | 答案 |
|---|---|---|
| 1 | 为什么门禁红？ | 文件头 3 字节是 `EF BB BF` |
| 2 | 为什么会有 BOM？ | 2026-10-17 的两笔合并提交把这些文件以 BOM 形式写回（`.editorconfig` 早已声明 `charset = utf-8`，说明落盘工具没遵守） |
| 3 | 为什么守卫没拦住？ | 守卫脚本（`b3c59268`，2026-10-01）只挂在 CI 的 `bom-guard` job，本地推送前没有任何等价检查 |
| 4 | 影响面多大？ | 全仓 1872 个 tracked 文件逐字节扫描：**仅这 5 个 `.rs`**，其他类型（`.toml`/`.md`/`.json` 等）零命中 |
| 5 | 怎么防复发？ | 把字节级守卫下沉到 `pre-push`（零依赖、秒级），并保留 CI job 作为最终门禁 |

### 3.3 修法：逐字节剥离，不做任何"顺手格式化"

```
crates/editor/dialog/src/window/initialize.rs   6836 → 6833 B
src/runner/cloud_progress.rs                   11385 → 11382 B
src/runner/device_warning.rs                    9244 → 9241 B
src/runner/progress_manager.rs                 11087 → 11084 B
src/runner/window_manager.rs                   17967 → 17964 B
```

**等价性校验**（不靠肉眼）：

```
# 工作区内容 == HEAD blob 去掉前 3 字节，逐字节相同
old = git show HEAD:<file>   # 以 BOM 开头
new = <worktree bytes>
assert old[:3] == b"\xef\xbb\xbf" and old[3:] == new     → 5/5 True
git diff --numstat                                        → 每文件 1/1（只动第 1 行）
```

---

## 4. 预防措施（流程固化，不靠个人记忆）

| 措施 | 内容 | 状态 |
|---|---|---|
| `.githooks/pre-push` | 推送前跑 `scripts/check-bom.sh`（字节级、零依赖、秒级）；`git config core.hooksPath .githooks` 启用，`git push --no-verify` 可跳过 | 新增 |
| 编译期契约测试 | `user_event_builder_matches_winit_api` 让 API 误用在全平台暴露 | 新增 |
| 本地 ubuntu 验证 SOP | 见 §6：Windows 开发机上用 WSL 跑 CI 同款命令 | 本次实践，写入本记录 |

---

## 5. 验证矩阵（每条都有命令与输出，非"我觉得"）

| 门禁 | 平台 | 命令 | 结果 |
|---|---|---|---|
| API 事实（反例） | 真 Ubuntu 26.04 | 最小 crate 复刻 CI 写法 | `E0599` 与 CI 原话逐字一致 ✅ |
| API 事实（正例） | 真 Ubuntu 26.04 | 同一 crate 改用 `EventLoop::<T>::with_user_event()` | 编译通过 ✅ |
| cargo check（CI 门禁 1） | 真 Ubuntu 26.04 | `cargo check --workspace --keep-going` | `Finished dev profile in 6m 06s`，0 error ✅ |
| cargo clippy（CI 门禁 2） | 真 Ubuntu 26.04 | `cargo clippy --workspace --all-targets -- -D warnings` | **RC=0**；全 workspace + all-targets 零 error 零 warning ✅ |
| BOM 守卫 | 真 Ubuntu 26.04 | `bash scripts/check-bom.sh` | `通过：已扫描 1565 个 .rs，无 BOM 命中` ✅ |
| BOM 复扫（全类型） | 真 Ubuntu 26.04 | 全仓 1872 文件逐字节 | 0 命中 ✅ |
| 格式 | Windows | `cargo fmt --check -- src/main.rs` | RC=0 ✅ |
| clippy（受影响 crate） | Windows | `cargo clippy -p lumino-rs --all-targets -- -D warnings` | RC=0，`Finished in 1m 09s` ✅ |
| 测试（受影响 crate） | Windows | `cargo test -p lumino-rs` | **本地未执行**：首个测试进程长时间未退出（见 §7-5），按流程交由 CI 三平台执行；本地仅确认测试目标可编译（clippy `--all-targets` 覆盖） ⚠️ |
| 检查语义自检 | 真 Ubuntu 26.04 | 最小 `lib+bin+build.rs` 探针，bin 内注入类型错误 | 报错精确命中 `src/main.rs:3` ⇒ `cargo check --workspace` **确实**检查 bin ✅ |
| 编译证据（终点） | 真 Ubuntu 26.04 | `touch src/main.rs && cargo check -p lumino-rs --bin lumino-rs -v` | 实打印 `rustc --crate-name lumino_rs --edition=2024 src/main.rs --crate-type bin …`，`Finished in 22.96s`，RC=0 ✅ |

### 5.1 读日志的坑（本次踩过，记录备查）

带 `build.rs` 的包，`cargo check` 对**整包只打印一行** `Compiling <pkg> v<ver> (<path>)`；
bin 的 check 单元复用这一行，**不会**再单独打印 `Checking <pkg> ... (bin)`。

后果：看到日志里没有 `Checking lumino-rs`，很容易误判成"根包 bin 根本没被检查"。实测探针
（`lib` + `bin` + `build.rs`，bin 内故意类型错误）：

```
   Compiling checkbin-probe v0.0.0 (/home/enderman/checkbin-probe)
error[E0308]: mismatched types
 --> src/main.rs:3:19
error: could not compile `checkbin-probe` (bin "checkbin-probe") due to 1 previous error
```

⇒ 只要 **RC=0 且日志内 0 个 `error`**，就该认为 bin 已被类型检查；不要靠状态行判断。

---

## 6. 本地 ubuntu 复现 SOP（Windows 开发机）

**为什么需要**：本仓有 **56 处**平台条件编译（macos 25 / windows 21 / linux 10）。任何一处
写错，都只会在对应平台的 CI job 上炸——Windows 本机永远看不见。S0 之后的每一次"平台相关"
改动，都建议按下面这条走一遍。

```bash
# WSL（Ubuntu 26.04）内执行；源码在 /mnt/d 上就地编译，产物落在 Linux 文件系统
cd /mnt/d/LuminoDevelop/Source/lumino-multi-workspace/<worktree>
export CARGO_TARGET_DIR="$HOME/ci-target"    # 避免 Windows 盘被 target 塞满
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=8

cargo check  --workspace --keep-going                    # CI lint 门禁 1
cargo clippy --workspace --all-targets -- -D warnings     # CI lint 门禁 2
bash scripts/check-bom.sh                                # CI bom-guard
```

实测成本：冷编译约 6 分钟（12 核 / 11 GB，首次需联网拉 crates.io）。

---

## 7. 遗留与风险（不藏）

1. **macOS job 无法本地复现**：本地只有 Windows + WSL Ubuntu，macOS 那 25 处 cfg 仍只能靠 CI；
2. **`--keep-going` 与 CI 有细微差异**：诊断时我加了它以便一次看全所有 Linux 侧错误；CI 是默认行为（遇到失败即停）。已验证无 error，差异不影响结论；
3. **BOM 落盘工具未定位到具体产品**：`.editorconfig` 已写 `charset = utf-8`、`.vscode/extensions.json` 已推荐 `editorconfig.editorconfig`，但 2026-10-17 的合并仍写回了 BOM。**怀疑对象是执行"合并 dev"这类批量提交的工具链**（而非 VS Code 日常保存）。下一步：在提交这类合并前跑 `git diff --cached --numstat` 抽查；守卫已下沉到 pre-push，复发会被当场拦住；
4. **推送者仍看不到 CI 结果**：本机无 `gh`，`api.github.com` 被网络策略挡。要么装 `gh` 用 `gh run watch`，要么用网页 Actions 页；这也是"红着推"的客观成因之一，值得单独治理；
5. **Windows 本地 `cargo test -p lumino-rs` 未跑完（本次实测记录）**：测试进程启动后约 10 分钟内只消耗 **6.0s CPU**、常驻 **634MB** —— 是阻塞不是计算。另有一个坑：中断 `cargo test` 只杀掉了外层 shell/cargo，**测试进程本身不退**（本次残留 PID 31152 占住 `target\debug\deps\lumino_rs-*.exe`，导致下一次链接报 `LNK1104: cannot open file`）。处置：本地不跑测试（按团队流程交 CI），残留进程已清理；**若 CI 的 windows test job 长时间无输出，优先怀疑是同一个测试在阻塞**。

---

## 8. 关联文档

- `CHANGELOG.md`（`0.2.1 Unreleased` → CI 修复条目）
- `.ai-context/sessions/2026-10/2026-10-18-ci-ubuntu-lint-and-bom-fix.md`
- `scripts/check-bom.sh`（守卫本体）、`.githooks/pre-push`（本地预检）

# 跨程序剪贴板音符同步 · 性能处理调研

> 范围：延续「跨程序本体剪贴板音符同步」方案，专攻**大规模选区（十万/百万/亿级音符）下
> 复制与粘贴的耗时与内存峰值**。
> 关联文档：`docs/research/clipboard-note-interop.md`（互操作方案）。
> 方法：逐路径读源码定位复杂度悬崖（事实驱动），给出按性价比排序的修复 + 基准验证方案。

---

## 0. 结论速览

| 级别 | 悬崖 | 位置 | 复杂度 | 规模量化（N 粘贴 / M 轨已有） |
|------|------|------|--------|-------------------------------|
| 🔴 P0 | 粘贴广播 O(N²) 全轨重扫 | `clipboard.rs:151` + `accessors.rs:181` | **O(N·M)** | 1M×1M ≈ 10¹² 次比较 |
| 🔴 P0 | 同上（走带路径也有） | `arrangement_ops/clipboard.rs:251` | **O(N·M)** | 同上 |
| 🟠 P1 | 复制全量物化 JSON（内存悬崖） | `clipboard.rs:25,30,45,67` | O(N) 但常量大 | 2.9 亿音符 → 2.3GB 索引 Vec + GB 级字符串 |
| 🟠 P1 | 走带复制全轨扫描 | `arrangement_ops/clipboard.rs:174` | O(tracks·notes) | 全选 = 全曲音符 |
| 🟡 P2 | 新增 SMF 二进制路径固有缩放 | `export.rs:17`→`tracks.rs` | O(N log N) + 2N 事件 | 二进制 ≈ JSON 的 1/10，但仍需裁剪/流式 |
| 🟢 OK | 粘贴后按参重选 | `selection.rs:128` | O(N·(logN+K)) | 窗口小，可接受 |

**最关键发现**：`doc.batch_insert_notes`（`document_edit.rs:90-94`）**在插入时已经分配了全局唯一 id**，
但只返回 `usize` 计数（`document_edit.rs:79` / `insert.rs:28`）。于是广播循环只能回退到
`note_id_at` 全轨线性重扫去「找回」id——这正是 O(N²) 的根因。修掉它（让批量插入返回 id 列表），
O(N²) 直接变 O(N)，且与跨程序功能无关、必须先做。

---

## 1. 🔴 P0：粘贴广播的 O(N²) 全轨重扫（两条路径都有）

### 1.1 钢琴卷帘路径
`crates/editor/ui-editor/src/clipboard.rs:143-170` 的 `commit_pasted_notes`：

```rust
self.editor_state.data.batch_insert_notes(&pasted);   // O(N+M) 归并，OK
for n in &pasted {
    let id = self.editor_state.data
        .note_id_at(track, n.tick, n.key)             // ← 对每 paste 音符调用一次
        .unwrap_or(0);
    emit(local_note_added(id, ...));                   // 协作广播
}
```

`note_id_at`（`crates/editor/editor-state/src/editor_state/editor_data/accessors.rs:181-195`）：

```rust
pub fn note_id_at(&self, track_id: usize, tick: f32, key: u16) -> Option<u64> {
    let notes = self.track_notes(track_id);
    let mut best = None;
    for n in notes.iter() {        // ← 全轨线性扫描，无提前退出
        let dt = (n.start_tick as f32 - tick).abs();
        if dt <= 1.0 && n.key as i32 - key as i32 == 0 { ... }
    }
    best.map(|b| b.1)
}
```

→ **每 paste 音符扫描整条轨（M 个音符）**，`for n in pasted` 共 N 次 ⇒ **O(N·M)**。

### 1.2 走带路径（同样问题）
`crates/editor/ui-editor/src/arrangement_ops/clipboard.rs:223-271` 的 `apply_paste_internal`：

```rust
for (...) in pasted {
    editor_data.insert_note(dest_track, note.clone());
    let id = self.editor_state.data
        .note_id_at(dest_track, note.tick, note.key)   // ← 同样 O(M) 重扫
        .unwrap_or(0);
    emit(local_note_added(id, ...));
}
```

> 注：走带这里是**逐条 `insert_note`**（O(M) 每次，N 次 ⇒ 本身也是 O(N·M) 插入），
> 比钢琴卷帘的 `batch_insert_notes` 更差。两条路径粘贴都吃 O(N²)。

### 1.3 规模量化
| 粘贴 N | 轨已有 M | O(N·M) 比较次数 | 现实后果 |
|--------|----------|------------------|----------|
| 1k | 1M | 10⁹ | 明显卡顿 |
| 10k | 1M | 10¹⁰ | 秒级冻结 |
| 100k | 1M | 10¹¹ | 数十秒 |
| 1M | 1M | 10¹² | 分钟级 / 疑似崩溃 |

黑 MIDI 场景 M 常达数千万~亿，N 也可能十万级 ⇒ 直接不可用。

---

## 2. 🔴 P0 根因 + 修复：让批量插入把 id 交出来

`doc.batch_insert_notes`（`document_edit.rs:79-108`）在插入时**已经**分配 id：

```rust
for n in notes.iter_mut() {
    if n.id == NoteEvent::UNASSIGNED_ID { n.id = self.allocate_note_id(); }
}
track_notes.extend_sorted(notes);
```

但它 `return inserted;` 只是计数（`document_edit.rs:107`）。`EditorData::batch_insert_notes`
（`insert.rs:16-38`）同样只返回 `usize`。

**修复（最高杠杆，O(N²)→O(N)）**：
1. 新增 `doc.batch_insert_notes_with_ids(...)`（或改签名）返回 `Vec<(u32 /*doc_index*/, u64 /*id*/)>`——
   在 `extend_sorted` 后按 `start_tick` 二分（ChunkedList `partition_point`，O(log M)）定位各新音符索引，
   组装 id 列表。总 O(N·log M)。
2. `EditorData::batch_insert_notes` 透传该列表（或返回 `Vec<(usize,u64)>`）。
3. `commit_pasted_notes` 用返回的 `(index,id)` 直接广播，**删除 `note_id_at` 循环**。
4. 走带 `apply_paste_internal` 改为先 `batch_insert_notes_to_track` 拿 id 列表再广播；
   同时把逐条 `insert_note` 换成批量插入，消除第二层 O(N·M)。

修复后粘贴复杂度：钢琴卷帘 O(N·log M)（归并 + 少量二分），走带同。
该修复**与跨程序 SMF 功能正交**，属必做项，应优先于任何互操作开发。

---

## 3. 🟠 P1：复制侧内存悬崖（全量物化 JSON）

`copy_selected_notes_to_clipboard`（`clipboard.rs:20-77`）：

```rust
let mut indices = self.get_selected_indices();        // Vec<usize>，全选=全部索引
let notes: Vec<&NoteEvent> = indices.filter_map(|i| current_track_notes().get(i)).collect();
let payload = serde_json::json!({ "notes": notes.iter().map(|n| json!({...})).collect::<Vec<_>>() });
clipboard.set_text(payload.to_string());              // 单 String ~ 50B × N
```

- `get_selected_indices`（`note_ops.rs:55-63`）对「全选」返回**全部索引的 Vec**（2.9 亿 → 2.3GB）。
- `Vec<&NoteEvent>` 再占 N×引用宽。
- `serde_json::json!` 构建 N 个 `Value` + `to_string()` 生成 GB 级字符串，再整段拷贝进系统剪贴板。

复杂度 O(N) 但常数巨大，内存峰值可达数 GB，**百万级以上复制即风险**。

**修复方向**：
- 流式写入：不复存整段 JSON，改为边遍历 `selection` 边写入 `Write`（或分块 `set_text` 追加）。
- 上限保护：超过阈值（如 50 万音符）给出「选区过大，建议导出 .mid」提示或自动降级。
- 跨程序 SMF 路径天然更小（见 §5），可用 SMF 替代 JSON 作为主载体降低体量。

### 3.1 走带复制全轨扫描
`collect_selected_notes_for_clipboard`（`arrangement_ops/clipboard.rs:174-195`）对每轨每音符
`selection.contains(visual_pos, tick, key)`——全选时 = 遍历全曲所有音符。O(tracks·notes)。
**修复**：按 selection 的 rect 反查命中音符（复用 `ChunkedList::window_range` 窗口查询，
`selection.rs:133` 已是 O(log N+K)），避免无差别全扫。

---

## 4. 🟢 OK：粘贴后按参重选
`select_notes_by_params`（`selection.rs:128-148`）对每个 pasted 音符用 `window_range`
（O(log N)）定位窗口再精确匹配，窗口小 ⇒ O(N·(log N+K))，**可接受**，无需改。
（保留它是因为副本 key 被 clamp 后可能与原件参数全等，须全选。）

---

## 5. 🟡 P2：新增 SMF 二进制路径的固有缩放

为跨程序互操作（`clipboard-note-interop.md` §5），复制时会额外生成 Standard MIDI File 字节。
序列化链路 `export_midi_to_bytes`（`export.rs:17`）→ `build_midi_smf`（`tracks.rs:11`）：

- 每音符 2 个 `TrackEvent`（NoteOn/NoteOff）⇒ 2N 事件，`Vec` 分配。
- `sort_by_key` ⇒ **O(N log N)**。
- `smf.write` 输出缓冲 ≈ N×2×~6B 增量编码。

与 JSON 文本比：二进制约为文本的 **1/10** 体量，显著提升大选区可行性；但 2.9 亿音符仍达 GB 级，
**必须与 §3 同样的裁剪/流式/上限策略**配套。且 SMF 写入也是 O(N log N)，在已修复 O(N²) 的前提下
不构成主瓶颈。

**建议**：SMF 生成走 `midly` 流式 `Smf::write` 到 `Cursor<Vec<u8>>`，必要时分轨/分块写；
超大选区与 JSON 共用同一上限保护。

---

## 6. 验证 / 基准方案（闭环证据）

### 6.1 微基准（证明 O(N·M) 与修复后 O(N·log M)）
在 `lumino-midi-model` 加 `#[bench]` 或临时 `tests`：
- 构造 M ∈ {1e4, 1e5, 1e6} 的 `MidiDocument` 单轨；
- 计时「对 N=1e4 个 paste 音符调用 `note_id_at` 找回 id」⇒ 应随 M 线性增长（印证 O(M)/次）。
- 修复后改为「`batch_insert_notes_with_ids` 一次取回」⇒ 计时应与 M 弱相关（仅 O(log M) 二分）。

### 6.2 集成基准
- 粘贴 1M 音符：修复前后 `cargo test` 计时 + 内存（`/usr/bin/time -v` 或 perf）。
- 复制 1M 音符：内存峰值对比（JSON vs SMF）。
- 断言：新增回归测试 `test_paste_no_quadratic`——构造大轨，粘贴后总比较次数 < 阈值（防 O(N²) 回潮）。

### 6.3 验收
- `cargo test -p lumino-midi-model -p lumino-editor-ui clipboard`
- Windows 实测：百万级选区复制/粘贴 ≤ 1s，内存峰值 < 500MB（SMF 路径）。

---

## 7. 优先级与排期

1. **P0（必做，先于互操作）**：批量插入返回 `(index,id)`，`commit_pasted_notes` / `apply_paste_internal`
   改用返回 id，删除 `note_id_at` 重扫；走带粘贴改批量插入。→ O(N²)→O(N·log M)。
2. **P1（复制侧）**：复制流式写 + 上限保护；走带复制按 rect 窗口反查。
3. **P2（互操作联动）**：SMF 路径复用 P1 上限，二进制体量为 JSON 的 1/10。

> 红线提醒：P0 与跨程序功能**解耦**——即便不做互操作，O(N²) 也已让大规模粘贴不可用，
> 应作为独立高优修复立即开工。

---

## 8. 修复状态（已实现，2026-09）

| 级别 | 修复 | 落地位置 | 验证 |
|------|------|----------|------|
| 🔴 P0 | 批量插入返回全局 id：`MidiDocument::batch_insert_notes_with_ids`；`commit_pasted_notes` / `apply_paste_internal` 改用返回 id 批量广播，删除 `note_id_at` 全轨重扫 | `midi-model/document_edit.rs`、`editor-state/note_store_ops/insert.rs`、`ui-editor/clipboard.rs`、`ui-editor/arrangement_ops/clipboard.rs` | `document_write_tests::test_batch_insert_with_ids_aligned_and_unique`（id 按输入序对齐、唯一）；走带 5 个粘贴回归测试全过 |
| 🟠 P1 | 复制流式写 JSON（避免 `Vec<Value>` GB 级分配）；走带复制按选区矩形窗口反查（`window_range`+id 去重），替代全轨扫描 | `ui-editor/clipboard.rs`、`ui-editor/arrangement_ops/clipboard.rs` | 走带粘贴回归测试全过 |
| 🟡 P2 | 跨程序 PPQN 一致性：复制载荷携带源 `division`，粘贴按 `ratio=目标/源` 对 tick 偏移与长度重采样（多一次同步计算），保证跨 Lumino 实例粘贴音符「长度与数据完全一致」；同 PPQN 时零缩放 | `ui-editor/clipboard.rs`（`parse_clipboard_notes`）、`ui-editor/arrangement_ops/clipboard.rs`（`parse_arrangement_clipboard_notes` / `write_arrangement_clipboard`） | `arrangement_ops::clipboard::test_paste_resamples_length_on_ppqn_mismatch`（480→960 重采样为 960）、`test_paste_no_resample_when_ppqn_matches`（一致不缩放） |

> 注：原 P2 方案为「接入 SMF 自定义剪贴板格式做跨 DAW 互拷」，已按用户纠正移除——本次范围仅限
> **Lumino 程序本体之间**同步，载体沿用 arboard 私有文本 JSON（天然跨进程），无需自定义二进制格式。

复杂度结论：P0 由 O(N·M) 降至 O(N·log M)（批量归并 + 一次广播）；P1 复制由「物化全量 `Vec<Value>` + 全选索引 Vec」改为流式 + 窗口反查；P2 重采样为每条音符一次定点乘法（O(N)），仅在 PPQN 不一致时触发。

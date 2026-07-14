# macOS音频引擎线程冲突分析

> **Workspace:** /Users/jieneng/Documents/GitHub/lumino-rs

---

**User**

整个彻查：macOS上，音频引擎（XSynth）似乎全程线程冲突，全都是卡卡的，到底是怎么一回事 
 我刚刚经手这个项目，对这个项目的结构不算特别清晰 
 如果你要了解更多，随时问我

**Assistant**

核心文件很多，让我先读取关键文件来理解音频架构。

让我继续看播放引擎核心和试听链路。

现在让我查看 xsynth 的 Cargo.toml 依赖配置，以及检查当前实际使用的是哪个版本的 xsynth。

好，彻查完毕。以下是完整诊断。

---

## macOS XSynth 音频卡顿根源分析

整个音频链路涉及 **5 层线程**，问题出在层与层之间的不协调，而非单一 bug。按严重程度排序：

---

### 致命级（直接导致卡卡声）

**1. xsynth-realtime 的 `BufferedRenderer::read()` 在音频回调中阻塞** — 这是头号元凶

[xsynth-issue-1](file:///Users/jieneng/Documents/GitHub/lumino-rs/docs/issues/xsynth-issue-1-blocking-recv-in-audio-callback.md) 已详细记录。`read()` 调用 `self.receive.recv().unwrap()` 会**阻塞 cpal 音频回调线程**。在 macOS 上 CoreAudio 回调有硬性时间约束（通常 ~5ms），一旦 `recv()` 阻塞超过这个阈值，音频 buffer underrun → 卡顿/爆音。

触发条件：任何时刻 render 线程没有及时产出数据，音频回调就会阻塞等待。这在初始化时、事件涌入时、系统调度抖动时都会发生。

**2. 播放线程 1ms 轮询 + 无节流的事件爆发**

[manager.rs:109-152](file:///Users/jieneng/Documents/GitHub/lumino-rs/crates/playback/src/manager.rs#L109-L152) — 播放线程每 1ms `thread::sleep` 一次，然后把所有到期事件一次性发到 xsynth 的 unbounded channel。问题：
- `thread::sleep(1ms)` 在 macOS 上实际精度是 **1-15ms**（取决于调度），导致 tick 计算不精确
- Seek 后重建事件队列，大量到期事件瞬间涌入 xsynth → 16 个 channel 渲染线程同时争抢 CPU → 渲染来不及 → 回调阻塞 → underrun

**3. 试听只有 `PlayNote`，几乎没有配对的 `StopNote`**

[editor_state.rs:127-131](file:///Users/jieneng/Documents/GitHub/lumino-rs/crates/core/src/editor_state.rs#L127-L131) — `play_note_audio()` 只入队 `AudioAction::PlayNote`。在整个编辑器代码中，`StopNote` 几乎没有生产路径（只有测试里有）。这意味着：
- 拖音、点音、反复预览 → voice 不断堆叠
- XSynth 默认每键 4 voice 上限 → 触发 voice stealing → 产生咔嗒声
- 积压的 voice 加重渲染负担 → 加剧 underrun

---

### 严重级（加剧卡顿，非根因）

**4. xsynth-realtime 16 线程开销**

xsynth-realtime 为每个 MIDI 通道创建独立渲染线程（16 个），加上 cpal 回调线程、播放线程 = 18+ 线程。macOS 的线程调度开销叠加 1ms 不精确唤醒，容易产生 buffer 缺口。

**5. 渲染循环每次迭代堆分配 Vec**

[xsynth-issue-2](file:///Users/jieneng/Documents/GitHub/lumino-rs/docs/issues/xsynth-issue-2-heap-allocation-in-render-loop.md) — 渲染线程每次迭代 `vec![Default::default(); size * channels]`（~700KB for 500ms@44100Hz stereo），增加非确定性延迟。

**6. XSynth 异步热切换不安全**

[midi_manager.rs:294-339](file:///Users/jieneng/Documents/GitHub/lumino-rs/src/runner/midi_manager.rs#L294-L339) — `check_async_init_complete()` 直接替换 `output`，而播放线程通过 `SetMidiOutput` 命令换上新 output。没有"停旧声部、同步当前活跃音、从当前 tick 重建状态"的逻辑。播放中切换 → 断音、错位 note_off、瞬态脉冲。

---

### 中等级

**7. 采样率设置形同虚设**

[xsynth.rs:93](file:///Users/jieneng/Documents/GitHub/lumino-rs/crates/midi-io/src/api/xsynth.rs#L93) — `open_with_default_output(rt_config)` 最终采样率由 cpal/设备决定。`XSynthOptions.sample_rate` 只用于音色库预加载，用户以为改了实际没改。

**8. Seek 无 Chase 机制**

Seek 到某个 tick 后，只发 `all_notes_off + reset_control`，不重放 seek 点之前的 CC/PC/PB 状态。比如 tick 300 设了 ProgramChange=5（电吉他），seek 到 500 后音色回退到默认钢琴。

**9. soundfont_cache 已修** — 缓存 key 已经包含 `(PathBuf, u32)` 采样率，这个之前的 issue 已解决。

---

## 线程架构全景图

```
[UI 线程 (egui/winit)]
  │  点击/拖拽 → AudioAction::PlayNote (没有配对 StopNote!)
  │  每帧 about_to_wait → process_audio_actions() → output.note_on()
  ▼
[Runner 主循环] ──── check_async_init_complete() → 热替换 output (不安全!)
  │
  ├─→ [播放线程] 1ms sleep 轮询
  │     │  BinaryHeap.pop() → MidiMessage
  │     │  out.note_on/off/cc/pc/pb → sender.send_event()
  │     │  sleep(1ms) ← 实际 1-15ms!
  │     ▼
  │   [xsynth-realtime 内部]
  │     │  unbounded channel 接收事件
  │     ├─→ [16 个通道渲染线程] ← 事件涌入时同时争抢 CPU
  │     │     render → Vec<f32> 堆分配 → send to buffer
  │     │     sleep(0.9 × render_window) ← 固定 sleep，无法追赶
  │     ▼
  │   [cpal/CoreAudio 回调线程] ← 硬性时间约束 ~5ms
  │         buffered.lock().read() ← recv() 可能阻塞!!! ← 头号元凶
  │         limiter → DAC → 扬声器
```

---

## 修复方案

### 短期（不改架构，显著降低卡顿）

| # | 措施 | 改动量 | 效果 |
|---|------|--------|------|
| 1 | **Fork xsynth，将 `BufferedRenderer::read()` 改为非阻塞** (`try_recv` + 静音填充) | xsynth-core 1 个方法 | **根治头号元凶**，回调永不阻塞 |
| 2 | **试听补全 `StopNote` 生命周期**：`play_note_audio` 同时启动定时器/在 `handle_released` 时发 `StopNote` | `editor_state.rs` + `interaction.rs` | 消除 voice 堆叠 |
| 3 | **禁止播放中热切换 XSynth**：`check_async_init_complete` 仅在 stopped/paused 状态时切换 | `midi_manager.rs` 约 5 行 | 消除断音/脉冲 |
| 4 | **暴露 `max_voices_per_key` 到 UI，默认调高到 8-16** | 已有配置项，改默认值 | 减少 voice stealing 咔嗒声 |
| 5 | **播放线程事件节流**：大量事件涌入时每批之间加 `spin_sleep(100μs)` | `manager.rs` 约 3 行 | 减缓 CPU 争抢峰值 |

### 中期（Chase 机制 + 事件模型升级）

| # | 措施 | 说明 |
|---|------|------|
| 6 | **Seek Chase 机制**：seek 时逆向扫描 CC/PC/PB，重放 seek 点前的最后状态 | 声音"听起来对"的前提 |
| 7 | **播放抽象升级为完整 MIDI 事件流**：当前 `OutputConnection` 已支持 CC/PC/PB（[xsynth.rs](file:///Users/jieneng/Documents/GitHub/lumino-rs/crates/midi-io/src/api/xsynth.rs) 里实现了），但 `AudioAction` 只有 PlayNote/StopNote，需要扩展 |

### 长期（架构重构，彻底解决）

| # | 措施 | 说明 |
|---|------|------|
| 8 | **放弃 xsynth-realtime，直接使用 xsynth-core + 自有 cpal 回调** | 参照 yueliang 的做法：在音频回调里先算 tick 发事件，再调 `read_samples()` 同步渲染，一切在一个调用栈里，零跨线程调度抖动 |
| 9 | **预分配 Vec 在渲染循环外** | 消除每次迭代的堆分配 |

---

### 建议优先级

**先做 #1 和 #2** — 这两项能解决 80% 的卡顿问题。#1 是根因（阻塞回调），#2 是诱因（voice 堆叠加重渲染负担）。

要我按哪个方案开始实施？

---

**User**

yinhe的性能做得十分优秀，你看看如何进行学习借鉴？ 
 yinhe是我亲自写的，而lumino是另一位开发者好朋友写的 
 /Users/jieneng/Documents/GitHub/yinhe

**Assistant**

好，让我深入分析 yinhe 的音频架构，和 lumino 对比找出可借鉴之处。

分析完毕。以下是 yinhe 和 lumino 的核心架构差异及可借鉴方案。

---

## yinhe vs lumino 音频架构对比

### 架构根本差异

| | **yinhe** | **lumino** |
|---|---|---|
| **合成器** | `xsynth-core::ChannelGroup` (直接控制) | `xsynth-realtime::RealtimeSynth` (黑盒封装) |
| **渲染方式** | **同步渲染** — 在 renderer 线程调 `channel_group.read_samples()` | **异步渲染** — xsynth-realtime 内部线程渲染，cpal 回调 `recv()` 取数据 |
| **cpal 回调** | 只做 `ring_buffer.pop_into()` + 静音填充，**永不阻塞** | `buffered.lock().read()` → 可能 `recv()` 阻塞 |
| **事件调度** | renderer 线程在 `render()` 中**先发事件再渲染**，同一调用栈 | 播放线程 1ms poll 发事件 → xsynth-realtime 内部 channel → 异步渲染 |
| **Chase** | seek 时 `inject_chase()` 重建 CC/PB/PC/RPN 状态 | seek 只发 `AllNotesOff + ResetControl`，不重建 |
| **离线导出** | 复用同一个 `AudioEngine` + `render()` | 独立的 export 模块 |

---

### yinhe 做对了什么（lumino 应该借鉴的 6 个关键点）

#### 1. 放弃 xsynth-realtime，直接用 xsynth-core — 这是零卡顿的根本原因

yinhe 的 [engine.rs](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/engine.rs) 只用了 `xsynth-core::ChannelGroup`，不用 `xsynth-realtime`。这意味着：
- 没有 `BufferedRenderer` 的阻塞 `recv()`
- 没有 16 个独立渲染线程争抢 CPU
- 没有 unbounded channel 的突发涌入问题
- 没有 render 线程的固定 sleep 调度抖动

yinhe 的渲染路径：**renderer 线程 → `dispatch_events()` → `channel_group.read_samples()`**，一切在一个调用栈里。

#### 2. 自研 lock-free SPSC ring buffer — cpal 回调永不阻塞

[audio_ring.rs](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/audio_ring.rs) — yinhe 自己实现了一个无锁环形缓冲区：
- Producer 在 renderer 线程写
- Consumer 在 cpal 回调读
- **回调里只做 `pop_into()`，如果缓冲区空就填静音，永远不会阻塞等待**

对比 lumino：cpal 回调走 `buffered.lock().read()` → `recv()` → 可能阻塞 → 卡顿。

#### 3. 三线程分离 — renderer + worker + cpal callback

[spawn.rs](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/spawn.rs) 的线程模型：
- **cpal 回调线程**：只做 `ring.pop_into()` + 静音填充，微秒级返回
- **renderer 线程**：处理命令、调 `engine.render()`、推数据到 ring buffer
- **worker 线程**：重活（model preparation、soundfont 加载）离线完成后发给 renderer

renderer 线程的调度循环（[audio_renderer.rs:76-88](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/audio_renderer.rs#L76-L88)）：
```
loop {
    did_work = process_commands() | process_worker_results() | render_if_needed()
    publish_state()
    if !did_work { sleep(1ms) }  // 只在无活时 sleep
}
```
关键区别：**有活干时不 sleep**，持续渲染填满 ring buffer；没活才 sleep。而 lumino 的播放线程每次都 sleep 1ms。

#### 4. sample-accurate 事件切分渲染

[engine_render.rs:12-52](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/engine_render.rs#L12-L52) — yinhe 的 `render()` 方法做了一件很精妙的事：

```rust
while rendered_until < block_end {
    let next_event_sample = self.next_event_sample(rendered_until, block_end);
    // 先渲染到下一个事件位置
    self.channel_group.read_samples(&mut output[start..end]);
    // 再在精确位置派发事件
    self.dispatch_cc_until(rendered_until);
    self.dispatch_notes_at(rendered_until);
}
```

在渲染一块音频时，**先找到下一个事件位置，渲染到那里，再发事件**。这保证了 CC/PB/NoteOn/NoteOff 在精确的 sample 位置生效，不会因为 1ms 轮询粒度产生时序错误。

对比 lumino：1ms 轮询 + 批量发送 → 事件可能在同一个 tick 内堆积，时机不精确。

#### 5. 完整的 Chase 机制

[engine_state.rs:298-317](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/engine_state.rs#L298-L317) + [channel.rs](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/channel.rs) — yinhe seek 时做三件事：
1. `AllNotesOff + ResetControl`
2. 逆向扫描 `cc_events`，收集每个通道最后的 CC/PC/PB/RPN 状态
3. `ChannelState::send_to()` 重放这些状态

而且 `ChannelState` 追踪了 **RPN 展开**（PBS、Fine Tune、Coarse Tune）和 generic CC values，确保 seek 后音色/弯音/踏板完全恢复。

对比 lumino：seek 后音色回退到默认钢琴、弯音归零、踏板状态丢失。

#### 6. 预分配 scratch buffer，零堆分配

[audio_renderer.rs:49](file:///Users/jieneng/Documents/GitHub/yinhe/crates/yinhe-audio/src/audio_renderer.rs#L49) — `scratch: vec![0.0; RENDER_CHUNK_FRAMES * STEREO_CHANNELS]` 在创建时分配一次，之后每帧重用。对比 xsynth-realtime 每次迭代分配 700KB Vec。

---

### Lumino 可以怎么做

按可行性和收益排序：

| 优先级 | 借鉴方案 | 效果 | 改动量 |
|--------|----------|------|--------|
| **P0** | 用 `xsynth-core::ChannelGroup` 替代 `xsynth-realtime`，自建 ring buffer + renderer 线程 | **根治所有卡顿** — 回调永不阻塞、无 16 线程争抢、无 recv 阻塞 | 大（但 yinhe 已有完整实现可参考） |
| **P1** | 实现 Chase 机制：seek 时重放 CC/PC/PB/RPN | seek 后声音正确 | 中（直接搬 yinhe 的 `ChannelState`） |
| **P2** | sample-accurate 事件切分渲染 | 消除 1ms 轮询的时序误差 | 中（搬 `engine_render.rs` 的 render 逻辑） |
| **P3** | 试听补全 StopNote 生命周期 | 消除 voice 堆叠 | 小 |
| **P4** | 禁止播放中热切换后端 | 消除断音 | 小 |
| **P5** | 离线导出复用同一引擎 | 导出和播放听感一致 | 中 |

**P0 是根本解决方案**。yinhe 证明了：用 `xsynth-core` 自己控制渲染 + ring buffer 解耦 cpal 回调，在 macOS 上零卡顿。其他所有 P1-P5 都是在 P0 架构上的自然延伸。

想从哪个优先级开始？如果做 P0，我可以按 yinhe 的架构为 lumino 设计 `yinhe-audio` 等价的模块。

---


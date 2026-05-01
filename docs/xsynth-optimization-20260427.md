我看完之后的判断很明确：**先不要急着弃用 `xsynth-realtime`**。你现在听到的爆音，更像是“上层播放链路 + 事件抽象 + 后端切换策略”几处问题叠在一起，不是 `xsynth-realtime` 一家背锅。现在直接全改成 `xsynth-core` 自己做实时播放，工程量很大，而且在你先修掉这些硬伤之前，收益未必最大。

关键证据我主要看了这几处：[XSynth 包装层](/Users/jieneng/Documents/GitHub/lumino-rs/crates/midi/src/api/xsynth.rs)、[播放管理器](/Users/jieneng/Documents/GitHub/lumino-rs/crates/ui/src/playback/manager.rs)、[编辑器试听事件](/Users/jieneng/Documents/GitHub/lumino-rs/crates/ui/src/editor/interaction.rs)、[播放初始化](/Users/jieneng/Documents/GitHub/lumino-rs/crates/ui/src/root/handlers/toolbar.rs)、[后端切换管理](/Users/jieneng/Documents/GitHub/lumino-rs/src/runner/midi_manager.rs)、[音色缓存](/Users/jieneng/Documents/GitHub/lumino-rs/crates/midi/src/soundfont_cache.rs)。`cargo check -p lumino-ui -p lumino-midi` 也能过，但有不少 unused/dead-code warning，说明这块子系统现在确实有点“过渡态”。

**现有软件里最要命的缺陷**

- **播放抽象太薄，只剩 NoteOn/NoteOff。**
  `OutputConnection` 只有 `note_on/note_off`，但你实际解析出来的 MIDI 事件已经有 `ControlChange / ProgramChange / Tempo / TrackName` 等完整信息了。结果播放层又把所有音都压成 `channel: 0`。这意味着 sustain、program change、pitch bend、打击乐通道这些都丢了。这个问题不管你用 `xsynth-realtime` 还是 `xsynth-core`，都会让“听起来不对”。

- **试听链路很可能只会发 `PlayNote`，不会发成对的 `StopNote`。**
  我在编辑器里看到多处 `PlayNote` 入队，但静态搜索没有看到对应的 `StopNote` 生产路径；`StopNote` 更像是“定义了但没真正用起来”。这会让拖音、点音、反复预览时不断堆叠活跃 voice，最后靠 voice stealing 或 panic 收场，特别容易听成爆音或脏尾音。这个是我目前最怀疑的直接原因之一。

- **当前播放是 1ms 轮询调度，不是真正和音频回调同时钟。**
  [playback manager](/Users/jieneng/Documents/GitHub/lumino-rs/crates/ui/src/playback/manager.rs) 每 1ms `sleep` 一次去吐 MIDI 事件，这在“编辑器里听个大概”还能工作，但它不是 sample-accurate，也很容易在高密度事件、系统调度抖动、seek/stop 瞬间出现扎堆发送。

- **停止/跳转时会暴力发 2048 个 `note_off`。**
  这虽然简单，但对实时队列很粗暴。更麻烦的是，你的抽象又没有 `AllNotesOff` / `ResetControl` / panic 这种更像 synth API 的能力，所以只能硬扫 16*128。

- **XSynth 异步热切换在播放中不安全。**
  `System -> XSynth` 初始化完成后，会直接把播放输出热替换掉；而播放线程收到 `SetMidiOutput` 只是把旧 output 换成新的，没有“停旧声部、同步当前活跃音、从当前 tick 重建状态”这套逻辑。正在播放时切过去，特别容易出现断音、错位 note_off、甚至瞬态脉冲。

- **“采样率”设置目前基本是假的。**
  你 UI 里能选 44.1k/48k/96k，但 `xsynth.rs` 实际是 `open_with_default_output(rt_config)`，最终 sample rate 还是设备默认值；`XSynthOptions.sample_rate` 没真正用到。用户会以为自己改了音质/性能，其实没改。

- **音色缓存 key 只按路径，不按 sample rate。**
  这个很隐蔽，但很危险。`SampleSoundfont::new(path, params, ...)` 是吃 `AudioStreamParams` 的，说明 soundfont 结果和输出采样率有关；你现在缓存只用 `PathBuf`，一旦设备采样率或未来离线渲染采样率变了，缓存就可能复用到不匹配的版本。

- **`fade_out_killing` 这个开关被 UI 描述得有点误导。**
  依赖里它实际是“被 voice limit 淘汰 / kill all voices 时是否淡出”，不是普通 `NoteOff` 的自然 release。所以它能减轻一部分点击声，但治不了你现在大多数播放链路问题。

- **你现在没改 `max_voices_per_key`，XSynth 默认每键最多 4 个 voice。**
  这对密集钢琴、同键快速重复、拖音试听都偏保守。超过上限时就会发生 voice stealing，而你正好又把试听做成了容易堆 voice 的路径，所以两边会互相放大问题。

**要不要现在就换成 `xsynth-core` 自己做实时播放？**

我的建议是：**现在不要一刀切换。先做“稳态化 + 抽象升级”，再决定是否替换实时后端。**

原因很简单：

1. `xsynth-realtime` 已经帮你搞定了 `cpal` 输出流、缓冲线程、limiter、stats、基础多线程，这部分重写成本不低。
2. 你现在最伤听感的几个点，都在它上面那层。
3. 真正值得统一的是“事件模型”和“音频引擎接口”，不是立刻把 `xsynth-realtime` 连根拔掉。

**我建议的路线**

1. **第一阶段：先把现有 realtime 路修稳。**
   修试听 note 生命周期；停止/seek 改成 synth 级 panic；禁止播放中热切换后端；把 `max_voices_per_key` 暴露出来，默认先放宽；把 XSynth stats 接出来，记录 `voice_count`、`average_renderer_load`、`last_samples_after_read`，先确认到底是 underrun 还是 voice stealing。

2. **第二阶段：把播放抽象从“音符输出”升级成“完整 MIDI 事件流”。**
   不再只传 `note_on/note_off`，而是支持 CC、program、pitch bend、all-notes-off、reset。编辑器自己的试听可以继续简单，但“导入 MIDI 后播放”必须走完整事件模型。否则你换任何 synth，结果都还是假的。

3. **第三阶段：做统一引擎，但先统一“接口”，不是先统一“实现”。**
   我会做一个 `AudioEngine`/`SynthEngine` 层：
   `realtime preview/playback` 先继续挂 `xsynth-realtime`；
   `offline render/export` 单独接 `xsynth-core`（或者参考 xsynth 的 render 实现）。
   这样你马上就能得到“离线渲染 / 预渲染 / 冻结导出”的能力，而且不会把实时播放一起炸掉。

4. **第四阶段：再决定要不要自研 realtime。**
   等前面三步做完，再比较两条路：
   继续用 `xsynth-realtime` 作为 live backend；
   或者改成 `xsynth-core + 自己的 cpal callback/buffered renderer`。
   到那时你有指标、有事件模型、有离线引擎，换不换就不是拍脑袋了。

**如果现在就要开工，我会先做这 5 项**

- 给 XSynth 包装层暴露 stats 和 panic 接口。
- 修掉试听只 `PlayNote` 不 `StopNote` 的生命周期问题。
- 暂停“播放中自动热切换到 XSynth”。
- 把 `max_voices_per_key` 做成配置项，默认值调高或直接 `None` 做验证。
- 修 soundfont cache key，把 `sample_rate` 至少纳入缓存维度；同时要么让采样率设置真正生效，要么先从 UI 隐掉。

一句话版建议：**短期别弃用 `xsynth-realtime`，中期要弃用的是你现在这层过薄的 MIDI/播放抽象。**
等这层抽象理顺了，`xsynth-core` 统一实时与离线渲染就会变成一条很顺的路，而不是一次高风险翻修。


基于对 yueliang 和 xsynth-realtime 源码的分析，下面是我发现的剩余隐患和可行的修复方向。

当前链路剩余爆音/卡顿根源
1. 双线程不协调（根本原因）
[事件线程 1ms poll]
    │  BinaryHeap.pop() → MidiMessage
    │  out.control_change() → sender.send_event()
    │  thread::sleep(1ms)  ← 不精确！实际可能是 1-15ms
    ▼
[xsynth-realtime 内部 16 通道线程 + 音频线程]
    │  接收事件 → 渲染 10ms 音频 → 发送 buffer → cpal 回调播放
事件线程 1ms 的 sleep 不能保证精确唤醒，OS 调度抖动可达 10-15ms。当大量事件在同一 tick 堆积（seek 后），会瞬间涌入 xsynth 的 unbounded channel，16 个通道线程同时争抢 CPU，导致音频 buffer 来不及就绪。

yueliang 的做法：不用 xsynth-realtime。直接用 xsynth-core::ChannelGroup。在 DAW 的 process() 回调里，先算 tick 发事件，再调 read_samples() 渲染。一切在一个调用栈里完成，无跨线程调度抖动。

2. xsynth-realtime 内部 16 线程开销
xsynth-realtime 为每个 MIDI 通道创建独立线程（realtime_synth.rs:164），加上音频输出线程、cpal 回调，共 18+ 个线程。每个线程 10ms 渲染窗口。在 macOS 上线程调度开销叠加我们的 1ms poll 线程，容易产生 buffer 缺口。

3. 缺少 Seek 后的 Chase 机制
当前 seek 后清空队列 rebuild、发 ResetControl，但不重放 seek 点之前的 CC/PC/PB 状态。例如 seek 到 tick 500，但 tick 300 设置了 ProgramChange=5（电吉他），seek 后音色会回退到默认钢琴。

yueliang 的 chase_events() 在 seek 时逆向扫描 256 个通道，找到 seek 点之前最后的 CC/PC/PB 值，重放一遍。这也是"声音听起来不对"的重要原因。

4. create_additional_output 的双实例风险
当前代码 strategy 2 会创建第二个 RealtimeSynth 实例（midi_manager.rs:354-367），等于同时运行两份 xsynth——两份音色加载、两份音频流。虽然 strategy 1 通常成功，但如果 fallback 触发就会双倍 CPU。

5. std::sync::Mutex 性能
PlaybackEngine.lock_playback() 每 1ms 调用一次，每次都要 lock std::sync::Mutex。在 macOS 上 parking_lot::Mutex 通常快 2-5 倍，减少锁争用带来的调度抖动。

建议的修复路线
🔴 短期（不改变架构，降低爆音概率）
措施	改动量	效果
防护双 xsynth 实例：strategy 2 对 xsynth 返回错误	midi_manager.rs 一行	避免 2× 音频流
parking_lot::Mutex 替代 std::sync::Mutex	playback/manager.rs + Cargo.toml	降低锁争用延迟
事件批量发送加 micro-sleep stagger	manager.rs 循环内加 spin_sleep::sleep(Duration::from_micros(100))	减缓事件涌入速度
暴露 voice_count 监控	xsynth.rs + 日志	能检测 overload
🟡 中期（Chase 机制，不用大改架构）
借鉴 yueliang 的 chase_events()，在 PlaybackEngine::seek() 时逆向扫描 midi_events，找到 seek 点前的最后 CC/PC/PB 值重放。

🟢 长期（架构重构，彻底解决）
参照 yueliang：放弃 xsynth-realtime，直接使用 xsynth-core::ChannelGroup，用自己的 cpal 回调做事件调度+音频渲染。这是 yueliang 在 macOS 上零爆音的根本原因。

xsynth-core 的 ChannelGroup API：

let mut group = ChannelGroup::new(config);
group.send_event(SynthEvent::Channel(0, ChannelEvent::Audio(...)));
group.read_samples(&mut buffer);  // 同步渲染，无需跨线程
要我按上面的路线开始修吗？先做短期措施（防护双实例 + parking_lot + voice_count 监控），还是直接做中期 Chase 机制？

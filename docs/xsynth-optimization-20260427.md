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

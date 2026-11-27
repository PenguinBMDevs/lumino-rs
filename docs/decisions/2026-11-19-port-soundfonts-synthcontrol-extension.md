# DEC-002：端口音色库切换走扩展 trait 默认方法（SynthControl）

- **状态**：生效（2026-11-19）
- **决策者**：产品负责人
- **关联卡**：REND-005（#88）、REND-002-CPU（#86）、REND-013（#107）、GPU 跟进卡（待建）、协作跟进卡（待建）
- **影响范围**：`lumino-midi-io` 的 `SynthControl` 扩展 trait、实时合成后端（XSynth/LGS）、`src/runner/midi_manager`

## 背景

REND-005（#88）要求“端口 → 音色库”映射在工程设置中修改后能作用到**正在运行**的实时合成器，同时满足：

- 不重建合成管线、不重开音频流；
- 不阻塞 UI 线程（#153 的整改方向）；
- 空闲时立即生效，播放中不产生断音/咔哒/错误 release。

`MidiManager` 持有 `Box<dyn SynthControl>`，没有具体后端类型可 downcast；若只靠“重建后端”应用映射，XSynth 需 100–300ms+ 且会重载音色库、重建输出连接，LGS 更重，属于体验与状态管理的倒退。

## 决策

1. **基础 trait 继续冻结**：`Api`、`OutputConnection` 不新增 required 方法，不改现有签名。
2. **能力进扩展 trait**：沿用 REND-002 的“接口演进 B”，在 `SynthControl`（`crates/audio/midi-io/src/lib.rs`）新增**带默认实现**的方法：

   ```rust
   /// 对正在运行的合成管线应用“端口 → 音色库”映射。
   ///
   /// 默认返回 Err（后端不支持）。XSynth 覆写为：
   /// - 通过 soundfont_cache 预加载去重后的 SF2；
   /// - 先 AllChannels(global)，再逐端口逐通道下发 SetSoundfonts；
   /// - 不重建管线、不重开音频流、不杀音。
   fn apply_port_soundfonts(
       &mut self,
       map: &PortSoundfontMap,
   ) -> Result<(), String> {
       Err(tr!("当前后端不支持按端口音色库").to_string())
   }
   ```

3. **默认实现即能力声明**：不支持的 XSynth 之外后端（LGS/System/Kdmapi/Maestro）保持默认 `Err`，由 Runner 转成可见提示，不要求任何现有后端改实现。
4. **运行期应用策略**：
   - 空闲（无活跃声部/已停播）：立即应用；
   - 播放中：工程态立即提交，音频层暂存，停播或声部归零后再切换，状态栏提示“将在当前播放结束后生效”；
   - 不调用 `set_midi_port_layout`、不发 `SystemReset`、不杀音；
   - SF2 解析全部在后台线程完成，UI 线程只提交期望值。
5. **边界**：
   - 只在扩展 trait 上加**默认方法**，不加 required 方法；
   - 不改现有方法签名；
   - 一次能力一个方法；若后续出现第二个端口级能力（bank/preset、端口效果等），优先改成一次传入的配置结构体，不再堆方法；
   - 不做具体后端 downcast，不让 Runner 依赖 XSynth 具体类型；
   - 本条**不是**“trait 可以随便加方法”的先例；任何 required 方法或基础 trait 改动都必须单独决策。

## 理由

| 维度 | 说明 |
|---|---|
| 体验 | 不重建、不断流；改映射不会中断播放 |
| 纪律 | 符合项目既定的“基础 trait 冻结、能力进扩展 trait”，与 #86 的 `set_midi_port_layout` 同型 |
| 兼容 | 默认方法对 5 个现有后端零改动，非 breaking |
| 架构 | 不泄漏具体后端类型；能力支持矩阵在代码里可见 |
| 状态 | 不重绑输出连接，不触发设备重开/重建竞态 |

## 后果（Consequences）

- XSynth 实现 `apply_port_soundfonts`；LGS 保持默认 `Err`，UI 需展示“GPU 后端暂不支持按端口音色库”；
- `PortSoundfontMap` 定义在 `lumino-midi-model`，导出/实时/工程/UI 共用；
- 播放中修改映射的切换时机由 `MidiManager` 的待应用状态与停播钩子控制；
- 决策记录：本文件；GitHub #88 显式提醒 Comment：
  <https://github.com/PenguinBMDevs/lumino-rs/issues/88#issuecomment-7001716256>

## 被否决的备选

1. **重建后端/重绑输出**：不碰 trait，但断流 100–300ms+、重载库、重建输出连接；与 #153 的 UI/播放流畅目标冲突。
2. **只在初始化时应用**：改映射必须重开工程/切后端，用户无即时反馈。
3. **新开 `PortSoundfontControl` trait**：`MidiApi = Box<dyn SynthControl>` 需要组合 trait object 或额外句柄，改动面更大，收益仅是“字面不碰已有 trait”。
4. **downcast 到具体 `XSynth`**：破坏抽象，Runner 依赖具体后端，不考虑。

## 修订方式

如需新增 required 方法、修改 `Api`/`OutputConnection`，或让本方法承担第二个能力，必须先修订本决策文档，说明边界与兼容性，再实施。

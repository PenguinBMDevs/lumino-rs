---
name: lumino-dsp-guard
description: "Lumino DSP 只读区与产品边界的开工门禁。当任务涉及 crates/audio/gpu-synth、crates/tools/export/src/audio、crates/audio/midi-io 的音频/DSP/合成器，或要求实现效果链、VST 宿主、路由矩阵、切换默认合成后端、修改播放/导出音频行为时触发。开工前读取 docs/2026-11-25-DSP-AI工作规则.md 与人读版，声明本次改动的 A/B/C 等级；触碰 A 级时停止落盘并按模板向用户请求确认。"
---

# Lumino DSP 只读区门禁

## 何时触发

- 改动或排查 `crates/audio/gpu-synth`、`crates/tools/export/src/audio` 下的代码；
- 涉及合成器后端（LGS / XSynth / Maestro / KDMAPI / 系统 MIDI）的行为；
- 需求里出现「效果、混响、合唱、延迟、CC91/93/94、VST、插件、路由、总线、母带」；
- 涉及播放/导出的音频输出、延迟、块长、复音、`VoiceState`、限幅。

## 开工三步

1. **读规则**：`docs/2026-11-25-DSP-AI工作规则.md`（纪律）与
   `docs/2026-11-25-DSP手搓区与AI禁区.md`（理由与案例）。`AGENTS.md` 的
   「DSP Read-Only Zone & AI Rules」是英文渲染版，两边冲突时以中文文档为准。
   若当前分支尚未合入这两份文档，用 `git show dev:docs/2026-11-25-DSP-AI工作规则.md`
   与 `git show dev:docs/2026-11-25-DSP手搓区与AI禁区.md` 读取。
2. **声明等级**：在第一次业务改动前，用一行说明本次改动属于 A / B / C 哪一级，
   以及是否触碰只读区。纯排查不需要声明等级，但结论里要写「未改动代码」。
3. **A 级即停**：需要改 A 级时，按规则第 4 节的模板向用户报告并等待明确同意；
   同意之前不落盘。B 级必须附对拍计划；C 级按仓库常规验证。

## 禁止

- 不告知用户就改 A 级；把 A 级改动夹带在「修复 / 格式化 / 清理警告」里；
- 未经产品决策实现效果链、VST 宿主、路由矩阵，或为「更专业」调整默认音色；
- 修改 xsynth fork（`PenguinBMDevs/xsynth-Lumino`），或复制其 LGPL 源码。

## 更愿意做的事

基准夹具、黄金哈希、`hash_render` / `lgs_ab.py` 诊断、只读回归测试、
数据层与控制面、把「为什么这么做」写进文档与卡面。

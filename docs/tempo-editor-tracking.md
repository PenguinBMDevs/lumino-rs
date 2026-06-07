# 速度编辑器（Conductor Track）— 进度跟踪

## Overview
Conductor track (track 0) protections + tempo editor in velocity panel position with triple toggle (Tempo → Global Volume → CC events).

## ✅ 已完成

- [x] **Conductor track note lock**: `finish_drawing()` 在 conductor track 时提前返回
- [x] **Conductor track delete protection**: `Track` 添加 `can_delete: bool`，track 0 为 `false`
- [x] **Tempo editor in velocity panel**: `EditMode::Tempo` 变体，显示名"速度"
- [x] **Tempo data model**: `TempoPoint { tick, bpm }` 加 `EditorData.tempo_points`
- [x] **Tempo rendering**: `draw_tempo_graph()` 折线图 + BPM 标签
- [x] **Toggle cycle**: 指挥轨道 Tempo → Cc(7) → Cc(selected_cc) → Tempo；普通轨道 Velocity ↔ Cc
- [x] **Tempo editing actions**: `TempoDragStart/Move/End/Add/Delete` 在 `VelocityAction` 中
- [x] **Playback sync**: `update_playback_bpm()` 同步到播放管理器
- [x] **Track switch auto-mode**: 选 conductor 自动切 Tempo，普通切 Velocity
- [x] **Document sync**: `set_midi_document()` 从 `doc.tempo_changes` 填充 `tempo_points`
- [x] **Startup fix**: `Root::new()` 和 `clear_editor()` 初始设 `edit_mode = Tempo`
- [x] **Tests**: 全部 164 项通过，无 clippy 新增警告

## ⬜ 未完成（Next Steps）

- [ ] **Tempo canvas interaction**: click 添加点，drag 改 BPM，right-click 删除 — 目前仅 display-only
- [ ] **Delete-track UI in sidebar**: 基于 `can_delete` 显示/隐藏删除按钮
- [ ] **Test tempo editing with real MIDI file playback**: 手动验证回放速度变化
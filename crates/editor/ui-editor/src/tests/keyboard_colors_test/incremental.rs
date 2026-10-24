//! 键盘颜色增量扫描：连续调用结果一致、无增长、与全量重扫一致

use super::*;

/// 验证增量扫描路径：连续调用 `update_playback_key_colors` 时，
/// 第二次以后走增量路径（而非全量重扫），结果必须与单次调用一致。
///
/// 覆盖以下路径：
/// 1. 首次调用 → 全量重扫（`notes_gen` 缓存）
/// 2. tick 前进 → 并入新开始音符（每键 `key_max_end` 单调取最大）
/// 3. 已结束音符自动「变暗」（`key_max_end <= tick` 即不输出颜色）
/// 4. tick 回退 → 触发全量重扫
#[test]
fn test_keyboard_colors_incremental_scan() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(doc);

    // === 阶段 1：tick=240，首次调用 → 全量重扫路径 ===
    // C4(0..480) 和 G4(0..1920) 活跃，E4(480..960) 未开始
    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be colored at tick 240 (full rescan)"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should be colored at tick 240 (full rescan)"
    );
    assert_eq!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should NOT be colored at tick 240"
    );

    // === 阶段 2：tick=480，tick 前进 → 增量扫描路径 ===
    // C4 刚结束（end=480），E4 刚开始（start=480），G4 仍活跃
    // 关键：C4 的 key_max_end 停在 480，不再 > tick → 必须自动变暗
    editor.playback_position = 480.0;
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be dark at tick 480 (end == tick)"
    );
    assert_ne!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should be added by incremental scan at tick 480"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should remain active at tick 480"
    );

    // === 阶段 3：tick=960，继续前进 → E4 自动变暗 ===
    // E4(480..960) 刚结束，G4(0..1920) 仍活跃
    editor.playback_position = 960.0;
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should be dark at tick 960"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should still be active at tick 960"
    );

    // === 阶段 4：tick=300，tick 回退 → 触发全量重扫 ===
    // 聚合被清空重算，C4 G4 应再次活跃
    editor.playback_position = 300.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be active again after seek backward (full rescan)"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should still be active after seek backward"
    );
    assert_eq!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should not be active at tick 300"
    );
}

/// 增量扫描结果必须与「全新状态一次性全量重扫」**逐字节一致**。
///
/// 这是本次重构（活跃集合 → 每键 `key_max_end` 聚合）的核心不变式：
/// 聚合对前缀单调取最大，正向推进与一次性重扫处理的是同一音符集合，
/// 结果必然相同。任何未来把「到期清理」加回来的改动都必须先满足本测试。
#[test]
fn test_keyboard_colors_incremental_matches_full_rescan() {
    let doc = make_test_doc();
    let mut incremental = Editor::new();
    incremental.playback_key_colors_enabled = true;
    incremental.editor_state.data.document = Some(doc);

    // 全量重扫参照：每轮前清空扫描状态，迫使走「notes_gen 为 None」的重扫分支
    let mut full = Editor::new();
    full.playback_key_colors_enabled = true;
    full.editor_state.data.document = Some(make_test_doc());

    for tick in 0..=2000u32 {
        incremental.playback_position = tick as f32;
        incremental.update_playback_key_colors();

        full.playback_scan_state = Default::default();
        full.playback_position = tick as f32;
        full.update_playback_key_colors();

        assert_eq!(
            incremental.playback_key_colors, full.playback_key_colors,
            "增量扫描与全量重扫结果不一致（tick={tick}）"
        );
    }
}

/// 聚合状态是**定长**的，不随播放进度增长。
///
/// 旧实现用 `active_notes: Vec<...>` 保存活跃集合，seek / 循环回绕时会瞬时
/// 涨到全曲规模（24 B/音符，高密度黑乐谱可达数百 MB）。新实现的聚合只有
/// `[u32; 256]` + `[[u8; 4]; 256]`，`scan_idx` 为 O(音轨数)，与播放进度无关。
#[test]
fn test_keyboard_colors_no_state_growth() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(doc);

    // 首个播放帧（tick=10）建立 O(音轨数) 的扫描位置。
    // 注意 tick=0 走「停止 → 清空」分支，会主动把扫描位置归零（正确行为），
    // 故循环起点从 tick=10 开始。
    editor.playback_position = 10.0;
    editor.update_playback_key_colors();
    let expected_scan_idx_len = editor.playback_scan_state.scan_idx.len();
    assert_eq!(expected_scan_idx_len, 2, "scan_idx 长度应等于文档音轨数");

    // 从 tick=10 连续推进到 tick=2000，每步 10 tick
    for tick in (10..=2000u32).step_by(10) {
        editor.playback_position = tick as f32;
        editor.update_playback_key_colors();
        assert_eq!(
            editor.playback_scan_state.key_max_end.len(),
            256,
            "每键聚合必须是定长 256 槽（tick={tick}）"
        );
        assert_eq!(
            editor.playback_scan_state.scan_idx.len(),
            expected_scan_idx_len,
            "scan_idx 长度只能是音轨数，不得随播放增长（tick={tick}）"
        );
    }
    // tick=2000 时所有音符已结束（最大 end=1920），颜色应全部熄灭
    assert_eq!(
        editor.playback_key_colors, [0u8; 1024],
        "所有音符结束后颜色应全部熄灭"
    );

    // 回退快照有上限（`SNAPSHOT_CAP = 64`），不得随播放无限增长
    assert!(
        editor.playback_scan_state.snapshots.len() <= 64,
        "快照数量必须有上限，实际 {}",
        editor.playback_scan_state.snapshots.len()
    );
}

/// 反向 seek / 循环回绕复用快照后，结果必须与「一次性全量重扫」逐字节一致。
///
/// 这是快照优化的核心不变式：快照保存的是「所有 `start_tick <= 快照点` 的音符」
/// 的前缀聚合，因此从任一 `<= 目标 tick` 的快照起算再补扫剩余区间，
/// 与从 tick 0 一次性扫到目标位置**必然等价**。
#[test]
fn test_keyboard_colors_backward_seek_matches_full_rescan() {
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(make_test_doc());

    // 正向播放一整遍，沿途采样快照
    for tick in (10..=1920u32).step_by(10) {
        editor.playback_position = tick as f32;
        editor.update_playback_key_colors();
    }
    assert!(
        !editor.playback_scan_state.snapshots.is_empty(),
        "正向播放应沿途记录回退快照"
    );

    // 反向 seek 到若干位置（含跨过多个音符边界），逐一与全量重扫比对
    for target in [1500u32, 1200, 961, 700, 480, 300, 240, 121, 61, 10] {
        editor.playback_position = target as f32;
        editor.update_playback_key_colors();

        let mut full = Editor::new();
        full.playback_key_colors_enabled = true;
        full.editor_state.data.document = Some(make_test_doc());
        full.playback_position = target as f32;
        full.update_playback_key_colors();

        assert_eq!(
            editor.playback_key_colors, full.playback_key_colors,
            "反向 seek 到 tick={target} 的结果与全量重扫不一致"
        );
    }
}

/// 扫描状态失效必须按**音符内容世代**判定，而不是文档内存地址。
///
/// 2026-10-17（审计 §五）：旧实现只比对 `doc as *const MidiDocument` 地址，
/// 而就地增删音符**不改变地址** → 状态被判为仍然有效，此后任何全量重扫
/// （seek / 循环回绕 / tick 回退）都会拿**陈旧数据**着色：
/// 已删除的音符仍然着色、新增音符不着色。
#[test]
fn test_keyboard_colors_state_follows_notes_generation() {
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(make_test_doc());

    // tick=240 时 C4(60) 与 G4(67) 活跃
    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_ne!(editor.playback_key_colors[60 * 4 + 3], 0, "C4 初始应着色");

    // 就地删除 C4（track 0 中 start_tick 最小者，即 index 0），并 bump 音符世代
    // ——所有直接改动音符的入口都必须 `mark_track_notes_changed`（marks.rs 契约）
    {
        let data = &mut editor.editor_state.data;
        let doc = data.document.as_mut().expect("文档应存在");
        assert!(doc.remove_note(0, 0).is_some(), "应删除 C4");
        data.mark_track_notes_changed();
    }

    // tick 回退 → 触发全量重扫：必须用**新内容**重算
    editor.playback_position = 120.0;
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "已删除的 C4 不得再着色：状态必须随音符世代失效并重扫"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 仍活跃，应保持着色"
    );
}

/// 装载新文档后（同一 tick、无 seek）也必须用**新内容**重扫。
///
/// 审计 §五 第二条：旧实现按文档内存地址判失效，而旧文档 drop 后新文档
/// **可能分配到同一地址** → 两份文档被判为同一份，残留上一份的键色。
/// 世代方案下「装载 = 世代变化」是硬前提，故这里按装载契约显式 bump
/// （与 `Root::set_midi_document` 的行为一致）。
#[test]
fn test_keyboard_colors_state_follows_document_replacement() {
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(make_test_doc());

    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "旧文档 C4 应着色"
    );

    // 替换为「同 tick 只有 D4(62)」的文档，并按装载契约 bump 世代
    editor.editor_state.data.document = Some(make_single_note_doc(62));
    editor.editor_state.data.mark_track_notes_changed();

    // 同一 tick（不落入 tick 回退分支）也必须触发重扫
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "旧文档的 C4 不得残留：状态必须随世代重扫"
    );
    assert_ne!(
        editor.playback_key_colors[62 * 4 + 3],
        0,
        "新文档的 D4 应着色"
    );
}

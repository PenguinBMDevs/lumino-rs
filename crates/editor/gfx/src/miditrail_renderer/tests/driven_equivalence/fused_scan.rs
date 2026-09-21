use super::*;

/// CPU 级：融合扫描（active＋aura）与 legacy 两次扫描逐位一致。
#[test]
fn test_fused_scan_matches_legacy() {
    let notes = dense_scene();
    let uniform = test_uniform();

    // legacy 输入：与 `render_from_instances` 逐 op 一致的换算。
    let derived: Vec<MiditrailNoteGpu> = notes
        .iter()
        .map(|n| {
            let (key, rgb) = crate::unpack_key_color(n.key_color);
            let start = n.start_length[0].max(0.0) as u32;
            let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
            MiditrailNoteGpu {
                key: key as u32,
                start_tick: start,
                end_tick: end,
                color_packed: crate::miditrail_renderer::pack_color([rgb[0], rgb[1], rgb[2], 1.0]),
                track_idx: 0,
                velocity: 100,
                channel: 0,
                _padding: 0,
            }
        })
        .collect();

    let expected_active = compute_active_keys(uniform.tick, &derived);
    let (actual_active, aura_sizes) = compute_active_and_aura_for_compact(
        uniform.tick,
        uniform.ticks_per_second,
        uniform.fps,
        &notes,
    );
    assert_eq!(
        expected_active.pressed, actual_active.pressed,
        "pressed 必须逐键一致"
    );
    assert_eq!(
        expected_active.colors, actual_active.colors,
        "激活颜色必须逐键一致"
    );

    // aura 实例逐位对比（legacy 全量扫描 vs 融合预聚合＋emit）。
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);
    let mut expected_auras = Vec::new();
    build_aura_instances(
        &uniform,
        &derived,
        &expected_active,
        &positions,
        &widths,
        &mut expected_auras,
    );
    let mut actual_auras = Vec::new();
    emit_aura_instances(
        &actual_active,
        &aura_sizes,
        uniform.key_count as usize,
        &positions,
        &widths,
        &mut actual_auras,
    );
    assert_eq!(
        expected_auras.len(),
        actual_auras.len(),
        "aura 实例数必须一致"
    );
    for (i, (a, b)) in expected_auras.iter().zip(actual_auras.iter()).enumerate() {
        assert_eq!(a.size, b.size, "第 {i} 个 aura 尺寸不一致");
        assert_eq!(a.pos, b.pos, "第 {i} 个 aura 位置不一致");
        assert_eq!(a.color_packed, b.color_packed, "第 {i} 个 aura 颜色不一致");
    }
}

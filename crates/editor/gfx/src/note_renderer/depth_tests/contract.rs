//! 区域化位空间深度 — CPU 孪生精度契约测试（从 depth_tests.rs 拆出）

use super::*;

/// 预览层（0.0）恒在主音轨区之前，主音轨区恒在洋葱皮区之前（全区间分层、无交叠）。
#[test]
fn test_layer_order_preview_before_main_before_onion() {
    assert!(
        0.0f32 < main_region_depth(0),
        "预览层（0.0）必须在主音轨区底之前"
    );
    let main_max = main_region_depth(DEPTH_REGION_SLOTS - 1);
    let onion_min = onion_region_depth(0);
    assert!(
        main_max < onion_min,
        "主音轨区顶 {main_max} 侵占了洋葱皮区底 {onion_min}"
    );
}

/// 区顶饱和前置：区底为 0 索引深度（无隐式偏移），区顶为槽数上界。
#[test]
fn test_region_depth_endpoints() {
    assert_eq!(main_region_depth(0), f32::from_bits(MAIN_DEPTH_REGION_BITS));
    assert_eq!(
        main_region_depth(DEPTH_REGION_SLOTS - 1),
        f32::from_bits(MAIN_DEPTH_REGION_BITS + DEPTH_REGION_SLOTS - 1)
    );
    assert_eq!(
        onion_region_depth(0),
        f32::from_bits(ONION_DEPTH_REGION_BITS)
    );
}

/// 区内深度严格单调（索引大者深度大）——采样覆盖项目 2.9 亿目标、
/// 本次实测黑乐谱规模（1936 万）以及旧实现的全部饱和边界。
#[test]
fn test_region_depth_strictly_increasing_with_project_scale() {
    for region in [MAIN_DEPTH_REGION_BITS, ONION_DEPTH_REGION_BITS] {
        for pair in SCALE_PROBE_INDICES.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (da, db) = (region_depth(region, a), region_depth(region, b));
            assert!(
                db > da,
                "区 {region:#010X} 索引 {a} → {b} 深度未严格递增（{da} → {db}）"
            );
        }
    }
}

/// 黑乐谱回归（本卡根因）：实测文件 1936 万音符、单轨最高 494 万，
/// 旧实现下这些索引全部落入饱和段共享深度；新实现必须全程唯一。
#[test]
fn test_black_midi_scale_indices_are_unique_in_region() {
    // 单个轨道段在缓冲中的索引区间示例（track 27：buffer [12_858_671, 17_801_051)）
    let track_start = 12_858_671u32;
    let track_len = 4_942_380u32;
    let mut prev = onion_region_depth(track_start);
    for idx in (track_start + 1)..(track_start + track_len) {
        let depth = onion_region_depth(idx);
        assert!(
            depth > prev,
            "索引 {idx} 深度未严格递增（旧实现在 209 万即饱和）"
        );
        prev = depth;
    }
}

/// chunk 折叠回归：不同 chunk 的同本地索引必须映射到不同深度
/// （旧实现直接用 chunk 局部索引算深度，多 chunk 时索引重置 → 深度别名：
/// 后画 chunk 的音符以「等深平局 + 后画者胜」反超，叠压关系与全局索引序相反）。
#[test]
fn test_chunk_folding_keeps_global_index_unique() {
    const CHUNK: u32 = 8_388_608; // 常见设备单 chunk 实例数（128MB binding / 16B）
    for local in [0u32, 1, 123, CHUNK - 1] {
        let a = global_index(0, local);
        let b = global_index(CHUNK, local);
        assert_ne!(a, b, "跨 chunk 同本地索引 {local} 的全局索引必须不同");
        assert!(
            onion_region_depth(b) > onion_region_depth(a),
            "后 chunk 的深度必须更大（同一区，全局索引序）"
        );
        assert!(
            main_region_depth(b) > main_region_depth(a),
            "主音轨区同样必须跨 chunk 连续"
        );
    }
}

/// 主音轨区与洋葱皮区对同一全局索引互不别名：主区恒在洋葱区之前。
#[test]
fn test_main_and_onion_regions_do_not_alias() {
    for idx in [0u32, 1, 6_291_455, 19_360_995, 290_000_000] {
        assert!(
            main_region_depth(idx) < onion_region_depth(idx),
            "索引 {idx}：主音轨区深度未小于洋葱皮区深度"
        );
    }
    assert!(main_region_depth(DEPTH_REGION_SLOTS - 1) < onion_region_depth(0));
}

/// 超出区槽数（5.2 亿，超出项目 2.9 亿目标规模）的极端索引饱和到区顶：
/// 确定性、有界、不越远平面。
#[test]
fn test_saturation_beyond_region_slots_is_bounded() {
    for region in [MAIN_DEPTH_REGION_BITS, ONION_DEPTH_REGION_BITS] {
        let saturated = region_depth(region, DEPTH_REGION_SLOTS - 1);
        for idx in [
            DEPTH_REGION_SLOTS,
            DEPTH_REGION_SLOTS + 1,
            u32::MAX - 1,
            u32::MAX,
        ] {
            assert_eq!(
                region_depth(region, idx),
                saturated,
                "区 {region:#010X} 索引 {idx} 未饱和到区顶"
            );
        }
        assert!(saturated < 1.0, "区顶 {saturated} 越出远平面");
    }
}

/// 任意深度都必须落在 NDC 远平面（z=1）之内，否则会被裁剪导致音符缺失。
#[test]
fn test_all_depths_stay_inside_far_plane() {
    for region in [MAIN_DEPTH_REGION_BITS, ONION_DEPTH_REGION_BITS] {
        for idx in [
            0u32,
            1,
            19_360_995,
            290_000_000,
            DEPTH_REGION_SLOTS - 1,
            u32::MAX,
        ] {
            let depth = region_depth(region, idx);
            assert!(
                (0.0..=1.0).contains(&depth),
                "区 {region:#010X} 索引 {idx} 深度 {depth} 越出 [0, 1]"
            );
        }
    }
}

/// 值域契约：区底 ≥ 最小正规格数（无 denormal，深度比较在所有后端稳定）；
/// 区顶 < 远平面 1.0。
#[test]
fn test_region_values_are_positive_normal_floats() {
    for region in [MAIN_DEPTH_REGION_BITS, ONION_DEPTH_REGION_BITS] {
        let min = region_depth(region, 0);
        let max = region_depth(region, DEPTH_REGION_SLOTS - 1);
        assert!(
            min.is_normal() && min > 0.0,
            "区 {region:#010X} 区底 {min} 必须是正规格数"
        );
        assert!(
            max.is_normal() && max < 1.0,
            "区 {region:#010X} 区顶 {max} 必须是正规格数且 < 1.0"
        );
    }
}

/// 4 个音符 shader 必须共享同一份深度契约（区域常量、区域函数、chunk 折叠逐字一致），
/// 旧「ulp 预算 + 局部索引」实现片段必须全部清除，
/// 并保留与 depth attachment 无关的静音轨退化几何裁剪。
#[test]
fn test_shader_sources_share_depth_contract() {
    const SOURCES: [(&str, &str); 4] = [
        ("note", include_str!("../../shaders/note.wgsl")),
        (
            "note_vertical",
            include_str!("../../shaders/note_vertical.wgsl"),
        ),
        ("onion_note", include_str!("../../shaders/onion_note.wgsl")),
        (
            "onion_note_vertical",
            include_str!("../../shaders/onion_note_vertical.wgsl"),
        ),
    ];
    const CONTRACT: [&str; 5] = [
        "const MAIN_DEPTH_REGION_BITS: u32 = 0x00800000u;",
        "const ONION_DEPTH_REGION_BITS: u32 = 0x1F800000u;",
        "const DEPTH_REGION_SLOTS: u32 = 0x1F000000u;",
        "fn region_depth(region_bits: u32, global_index: u32) -> f32 {",
        "let global_index = chunk_info.chunk_start + visible_index;",
    ];
    for (label, source) in SOURCES {
        for needle in CONTRACT {
            assert!(
                source.contains(needle),
                "{label}.wgsl 缺少深度契约片段：{needle}"
            );
        }
        for forbidden in [
            "tie_break_depth",
            "MAIN_TRACK_DEPTH_BASE",
            "TRACK_DEPTH_STEP",
            "TIE_BREAK_INDEX_LIMIT",
        ] {
            assert!(
                !source.contains(forbidden),
                "{label}.wgsl 仍残留旧实现片段：{forbidden}"
            );
        }
        assert!(
            !source.contains("vec4<f32>(0.0, 0.0, 2.0, 1.0)"),
            "{label}.wgsl 仍在使用依赖 depth attachment 的 NDC z=2.0 静音裁剪"
        );
    }
}

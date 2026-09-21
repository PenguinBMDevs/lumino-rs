use lumino_message::events::window::video::CounterFont;

use super::super::counter_font_ttf_load::{load_font_bytes, system_font_path};
use super::*;

fn msyh_path() -> std::path::PathBuf {
    std::path::PathBuf::from("C:\\Windows\\Fonts\\msyh.ttc")
}

/// 系统字体路径表：Windows 下微软雅黑可解析
#[test]
fn test_system_font_path_windows() {
    #[cfg(target_os = "windows")]
    {
        assert!(
            system_font_path("微软雅黑").is_some(),
            "Windows 应有微软雅黑路径"
        );
        assert!(
            system_font_path("不存在的字体").is_none(),
            "未知字体应返回 None"
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows 平台：不保证路径存在，只验证不 panic
        let _ = system_font_path("微软雅黑");
    }
}

/// TTC 集合加载：微软雅黑（多 face 集合）应能解析第 0 个 face
#[test]
fn test_load_ttc_collection() {
    if !msyh_path().is_file() {
        eprintln!("跳过：系统缺少 msyh.ttc");
        return;
    }
    let bytes = std::fs::read(msyh_path()).expect("读取 msyh.ttc");
    assert!(load_font_bytes(bytes).is_ok(), "TTC 集合应可加载");
}

/// 无效文件 → Err
#[test]
fn test_load_invalid_font_fails() {
    let res = load_font_bytes(vec![0u8; 64]);
    assert!(res.is_err(), "无效字体字节应报错");
}

/// 中文字符渲染：微软雅黑绘制「音符」出现非零像素
#[test]
fn test_draw_chinese_chars() {
    if !msyh_path().is_file() {
        eprintln!("跳过：系统缺少 msyh.ttc");
        return;
    }
    let mut r = TtfFontRenderer::new(
        &CounterFont::System {
            family: "微软雅黑".to_string(),
        },
        16,
    )
    .expect("加载微软雅黑");
    let mut frame = vec![0u8; 64 * 32 * 4];
    r.draw_line(&mut frame, 64, "音符", 0, 0, [255, 255, 255, 255]);
    let white_count = frame.as_chunks::<4>().0.iter().filter(|p| p[0] > 0).count();
    assert!(white_count > 0, "中文应渲染出像素，实际 {white_count}");
}

/// 中文测量宽度 > 0
#[test]
fn test_measure_chinese() {
    if !msyh_path().is_file() {
        eprintln!("跳过：系统缺少 msyh.ttc");
        return;
    }
    let mut r = TtfFontRenderer::new(
        &CounterFont::System {
            family: "微软雅黑".to_string(),
        },
        16,
    )
    .expect("加载微软雅黑");
    assert!(r.measure_line("音符") > 0);
    assert!(r.measure_line("ABC") > 0);
}

/// glyph 缓存：重复字符只光栅化一次
#[test]
fn test_glyph_cache_hits() {
    if !msyh_path().is_file() {
        eprintln!("跳过：系统缺少 msyh.ttc");
        return;
    }
    let mut r = TtfFontRenderer::new(
        &CounterFont::System {
            family: "微软雅黑".to_string(),
        },
        16,
    )
    .expect("加载微软雅黑");
    let mut frame = vec![0u8; 64 * 32 * 4];
    r.draw_line(&mut frame, 64, "音", 0, 0, [255, 255, 255, 255]);
    let size_after_first = r.cache.len();
    r.draw_line(&mut frame, 64, "音", 0, 0, [255, 255, 255, 255]);
    assert_eq!(r.cache.len(), size_after_first, "重复字符不应重新光栅化");
    r.draw_line(&mut frame, 64, "符", 0, 0, [255, 255, 255, 255]);
    assert_eq!(r.cache.len(), size_after_first + 1, "新字符应新增缓存");
}

/// 字体缺失字符（如「𠀀」生僻字）：不 panic，按空格推进
#[test]
fn test_missing_glyph_no_panic() {
    if !msyh_path().is_file() {
        eprintln!("跳过：系统缺少 msyh.ttc");
        return;
    }
    let mut r = TtfFontRenderer::new(
        &CounterFont::System {
            family: "微软雅黑".to_string(),
        },
        16,
    )
    .expect("加载微软雅黑");
    let mut frame = vec![0u8; 64 * 32 * 4];
    r.draw_line(&mut frame, 64, "𠀀", 0, 0, [255, 255, 255, 255]);
    // 缺失 glyph 按空格宽度推进（>0），不 panic
    assert!(r.measure_line("𠀀") > 0);
}

/// 越界绘制不 panic（行顶为负/超出帧高）
#[test]
fn test_out_of_bounds_no_panic() {
    if !msyh_path().is_file() {
        eprintln!("跳过：系统缺少 msyh.ttc");
        return;
    }
    let mut r = TtfFontRenderer::new(
        &CounterFont::System {
            family: "微软雅黑".to_string(),
        },
        16,
    )
    .expect("加载微软雅黑");
    let mut frame = vec![0u8; 64 * 32 * 4];
    r.draw_line(
        &mut frame,
        64,
        "中文测试",
        u32::MAX,
        0,
        [255, 255, 255, 255],
    );
    r.draw_line(
        &mut frame,
        64,
        "中文测试",
        0,
        u32::MAX,
        [255, 255, 255, 255],
    );
}

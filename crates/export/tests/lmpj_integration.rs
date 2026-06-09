use std::path::PathBuf;

/// 集成测试：如果通过 `LUMINO_TEST_LMPJ` 环境变量提供了可访问的 LMPJ 文件，
/// 则测试解码并使用 `save`/`save_sync` 写回临时文件再解码。
#[test]
#[ignore = "需要外部 LMPJ 文件，设置 LUMINO_TEST_LMPJ 环境变量"]
fn decode_and_resave_lmpj_if_present() {
    let candidates = vec![std::env::var("LUMINO_TEST_LMPJ").ok().map(PathBuf::from)];

    let path = candidates.into_iter().flatten().find(|p| p.exists());
    if path.is_none() {
        eprintln!("跳过 LMPJ 集成测试：未找到测试文件（可设置 LUMINO_TEST_LMPJ 环境变量）");
        return;
    }
    let path = path.unwrap();

    // 尝试解码为 ParsedMidi
    let bytes = std::fs::read(&path).expect("读取 LMPJ 失败");
    let parsed: lumino_midi_loader::ParsedMidi =
        lumino_export::format::decode_lmpj(&bytes).expect("解码 LMPJ 失败");

    // 现在把它保存到临时文件
    let mut tmp = std::env::temp_dir();
    tmp.push("lumino_test_resave.lmpj");

    // 使用同步保存，确保不依赖 tokio runtime
    lumino_export::save_sync(&parsed, &tmp).expect("保存到临时 LMPJ 失败");

    // 再次读取并解码，验证基本字段
    let round_bytes = std::fs::read(&tmp).expect("读取临时 LMPJ 失败");
    let parsed_round: lumino_midi_loader::ParsedMidi =
        lumino_export::format::decode_lmpj(&round_bytes).expect("解码临时 LMPJ 失败");

    // 验证基本信息一致（path 字段可能相同）
    assert_eq!(parsed.info.track_count, parsed_round.info.track_count);
    assert_eq!(parsed.info.total_notes, parsed_round.info.total_notes);

    // 清理临时文件
    let _ = std::fs::remove_file(&tmp);
}

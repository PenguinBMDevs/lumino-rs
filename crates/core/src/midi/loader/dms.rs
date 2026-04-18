use std::path::PathBuf;

use crate::{DmsInfo, ParsedDms};

use super::types::ProgressCallback;

pub async fn load_dms(
    path: PathBuf,
    progress: Option<&ProgressCallback>,
) -> crate::Result<ParsedDms> {
    let cb = |msg: &str, val: f64| {
        if let Some(p) = progress {
            p(msg, val);
        }
    };

    cb("正在准备加载 Domino 工程文件", 0.0);
    cb("正在打开 Domino 工程文件", 0.05);

    tracing::info!("[DMS加载] 开始加载文件: {:?}", path);

    let path_clone = path.clone();
    let progress_clone = progress.cloned();
    let (scan_result, lightweight_data) = tokio::task::spawn_blocking(move || {
        puffin::profile_scope!("load_dms_blocking");

        let pcb = progress_clone.as_ref();
        let scan_cb = |msg: &str, val: f64| {
            if let Some(p) = pcb {
                p(msg, val);
            }
        };

        // 首先流式扫描获取元数据
        tracing::info!("[DMS加载] 步骤1: 打开文件");
        let file = std::fs::File::open(&path_clone).map_err(crate::CoreError::Io)?;
        let mut reader = std::io::BufReader::new(file);
        tracing::info!("[DMS加载] 步骤2: 开始流式扫描");
        let scan_result = lumino_dms::scan_dms_streaming_with_progress(&mut reader, |progress| {
            scan_cb("正在解析 Domino 工程文件", 0.1 + progress * 0.4);
        })
        .map_err(|e| crate::CoreError::FileFormat(format!("扫描 DMS 失败: {e}")))?;
        tracing::info!(
            "[DMS加载] 步骤3: 扫描完成, 轨道数={}, 音符数={}",
            scan_result.track_count,
            scan_result.total_notes
        );

        // 然后加载完整数据
        scan_cb("正在加载完整数据", 0.5);
        tracing::info!("[DMS加载] 步骤4: 读取完整文件数据");
        let bytes = std::fs::read(&path_clone).map_err(crate::CoreError::Io)?;
        tracing::info!("[DMS加载] 步骤5: 文件大小 {} 字节", bytes.len());

        tracing::info!("[DMS加载] 步骤6: 解压 DMS 数据");
        let lightweight_data = lumino_dms::read_dms_lightweight(&bytes)
            .map_err(|e| crate::CoreError::FileFormat(format!("读取 DMS 数据失败: {e}")))?;
        tracing::info!(
            "[DMS加载] 步骤7: 解压完成, 解压后大小 {} 字节",
            lightweight_data.len()
        );

        scan_cb("数据加载完成", 0.9);

        Ok::<_, crate::CoreError>((scan_result, lightweight_data))
    })
    .await
    .map_err(|e| {
        let err = crate::CoreError::Other(format!("加载 DMS 失败: {e}"));
        tracing::error!("[DMS加载] 任务执行失败: {}", e);
        cb(&err.to_string(), 1.0);
        err
    })?
    .map_err(|e| {
        let err = crate::CoreError::Compression(format!("处理 DMS 失败: {e}"));
        tracing::error!("[DMS加载] 数据处理失败: {}", e);
        cb(&err.to_string(), 1.0);
        err
    })?;

    cb("Domino 工程文件加载完成", 1.0);
    tracing::info!("[DMS加载] 加载完成成功");

    let info = DmsInfo {
        path,
        song_name: scan_result.song_name,
        copyright: scan_result.copyright,
        comment: scan_result.comment,
        ppqn: scan_result.ppqn,
        track_count: scan_result.track_count,
        total_notes: scan_result.total_notes,
        working_time_sec: scan_result.working_time_sec,
    };

    Ok(ParsedDms {
        info,
        data: Some(lightweight_data),
    })
}

pub async fn save_dms_to_ldms(
    parsed: &ParsedDms,
    path: PathBuf,
    progress: Option<&ProgressCallback>,
) -> crate::Result<()> {
    let cb = |msg: &str, val: f64| {
        if let Some(p) = progress {
            p(msg, val);
        }
    };

    cb("准备保存 LDMS", 0.0);
    cb("保存 LDMS", 0.1);

    let data_for_save = ParsedDms {
        info: parsed.info.clone(),
        data: None,
    };

    let data = bincode::serialize(&data_for_save).map_err(|e| {
        let err = crate::CoreError::from(e);
        cb(&err.to_string(), 1.0);
        err
    })?;

    cb("压缩 LDMS", 0.4);
    let compressed = tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(data);
        zstd::stream::encode_all(cursor, 3)
    })
    .await
    .map_err(|e| {
        let err = crate::CoreError::Other(format!("压缩 LDMS 失败: {e}"));
        cb(&err.to_string(), 1.0);
        err
    })?
    .map_err(|e| {
        let err = crate::CoreError::Compression(format!("压缩 LDMS 失败: {e}"));
        cb(&err.to_string(), 1.0);
        err
    })?;

    tokio::fs::write(&path, compressed).await.map_err(|e| {
        let err = crate::CoreError::Io(e);
        cb(&err.to_string(), 1.0);
        err
    })?;

    cb("LDMS 保存完成", 1.0);
    Ok(())
}

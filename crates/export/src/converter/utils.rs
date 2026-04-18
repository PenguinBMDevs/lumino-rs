use std::path::Path;

pub fn copy_file_sync(source_path: &Path, save_path: &Path) -> Result<u64, String> {
    std::fs::copy(source_path, save_path).map_err(|e| format!("复制文件失败: {e}"))
}

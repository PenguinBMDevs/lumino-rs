use lumino_core::storage::ui_state::*;
use std::{fs, io, path::PathBuf};

#[derive(Debug)]
pub struct UiStateWrapper {
    inner: UiState,
    path: PathBuf,
    dirty: bool,
}

impl UiStateWrapper {
    pub fn new(path: PathBuf) -> Self {
        let inner = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(state) => state,
                Err(e) => {
                    tracing::warn!("UI 状态文件解析失败 ({}), 使用默认状态", e);
                    UiState::default()
                }
            },
            Err(_) => UiState::default(),
        };
        Self {
            inner,
            path,
            dirty: false,
        }
    }
    pub fn get(&self) -> &UiState {
        &self.inner
    }
    pub fn patch<F>(&mut self, f: F)
    where
        F: FnOnce(&mut UiState),
    {
        f(&mut self.inner);
        self.dirty = true;
    }
    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(&self.path)?;
        let writer = io::BufWriter::new(file);
        serde_json::to_writer(writer, &self.inner)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.dirty = false;
        Ok(())
    }
}

use std::{fs, io, path::PathBuf};
use lumino_core::storage::config::*;

#[derive(Debug)]
pub struct ConfigWrapper {
    inner: Config,
    path: PathBuf,
    dirty: bool,
}

impl ConfigWrapper {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let inner = if path.exists() {
            let bytes = fs::read(&path)?;
            toml::from_slice(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            Config::default()
        };
        Ok(Self {
            inner,
            path,
            dirty: false,
        })
    }
    pub fn get(&self) -> &Config {
        &self.inner
    }
    pub fn patch<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Config),
    {
        f(&mut self.inner);
        self.dirty = true;
    }
    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let str = toml::to_string_pretty(&self.inner)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.path, str.as_bytes())?;
        self.dirty = false;
        Ok(())
    }
}

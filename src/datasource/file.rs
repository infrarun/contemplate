use super::{Notifier, Source};
use crate::datasource::{Result, ToDataSourceError};
use crate::error::Error;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use figment::{
    providers::{Format, Json, Toml, YamlExtended},
    Figment,
};

use notify::{Config, EventKind, RecommendedWatcher, Watcher};

pub struct File {
    path: PathBuf,
    watcher: Option<RecommendedWatcher>,
}

impl File {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref().to_owned();
        Self {
            path,
            watcher: None,
        }
    }
}

#[async_trait]
impl Source for File {
    async fn merge_to_figment(&self, figment: Figment) -> Result<Figment> {
        let figment = match self
            .path
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .as_deref()
        {
            Some("json") => figment.merge(Json::file(&self.path)),
            Some("toml") => figment.merge(Toml::file(&self.path)),
            Some("yaml") | Some("yml") => figment.merge(YamlExtended::file(&self.path)),
            Some(ext) => {
                return Err(Error::UnknownFileExtension {
                    ext: ext.to_owned(),
                })
                .fatal()
            }
            None => {
                return Err(Error::UnknownFileType {
                    path: self.path.to_owned(),
                })
                .fatal()
            }
        };

        Ok(figment)
    }

    async fn watch(&mut self, notify: Notifier) {
        let self_dbg = format!("{:?}", *self);
        let Ok(mut watcher) = RecommendedWatcher::new(
            move |evt: std::result::Result<notify::Event, notify::Error>| match evt {
                Ok(e) => {
                    if matches!(
                        e.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) {
                        notify.notify(&self_dbg);
                    }
                }
                Err(e) => {
                    log::warn!("Notify error: {e}")
                }
            },
            Config::default(),
        ) else {
            log::error!("Could not create notifier for {:?}", self.path);
            return;
        };

        if let Err(e) = watcher.watch(&self.path, notify::RecursiveMode::NonRecursive) {
            log::error!("Could not register notifier for {:?}: {e}", self.path)
        }

        self.watcher.replace(watcher);
    }
}

impl std::fmt::Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File").field("path", &self.path).finish()
    }
}

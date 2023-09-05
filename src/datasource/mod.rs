use std::{
    fmt::{Debug, Display},
    pin::Pin,
};

use async_trait::async_trait;
use figment::Figment;
mod file;
pub use file::File;

mod env;
pub use env::Environment;

pub mod k8s;
pub use k8s::ConfigMap;

use tokio::sync::mpsc;

pub struct Notifier {
    tx: mpsc::Sender<()>,
}

impl Notifier {
    pub fn new(tx: mpsc::Sender<()>) -> Self {
        Self { tx }
    }

    /// Notify that the underlying storage has changed.
    pub fn notify<S>(&self, source: &S)
    where
        S: Display,
    {
        if let Err(e) = self.tx.blocking_send(()) {
            log::warn!("Error sending notify event: {e}");
        }
        log::info!("Reload triggered by {source}");
    }

    pub async fn notify_async<S>(&self, source: &S)
    where
        S: Display,
    {
        if let Err(e) = self.tx.send(()).await {
            log::warn!("Error sending notify event: {e}");
        }

        log::info!("Reload triggered by {source}");
    }
}

use crate::error::Error;

pub enum DataSourceError {
    Recoverable(Error),
    Fatal(Error),
}

impl DataSourceError {
    pub fn recoverable(e: Error) -> Self {
        Self::Recoverable(e)
    }

    pub fn fatal(e: Error) -> Self {
        Self::Fatal(e)
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::Recoverable(_))
    }

    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }
}

impl From<DataSourceError> for crate::error::Error {
    fn from(value: DataSourceError) -> Self {
        match value {
            DataSourceError::Recoverable(e) => e,
            DataSourceError::Fatal(e) => e,
        }
    }
}

impl AsRef<Error> for DataSourceError {
    fn as_ref(&self) -> &Error {
        match self {
            DataSourceError::Recoverable(e) => e,
            DataSourceError::Fatal(e) => e,
        }
    }
}

impl std::ops::Deref for DataSourceError {
    type Target = Error;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl Display for DataSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

pub type Result<T> = std::result::Result<T, DataSourceError>;

pub trait ToDataSourceError {
    type Target;
    fn fatal(self) -> Self::Target;
    fn recoverable(self) -> Self::Target;
}

/*
impl ToDataSourceError for Error {
    type Target = DataSourceError;

    fn fatal(self) -> Self::Target {
        DataSourceError::Fatal(self)
    }

    fn recoverable(self) -> Self::Target {
        DataSourceError::Recoverable(self)
    }
} */

impl<E, T> ToDataSourceError for std::result::Result<T, E>
where
    E: ToDataSourceError,
{
    type Target = std::result::Result<T, E::Target>;

    fn fatal(self) -> Self::Target {
        self.map_err(ToDataSourceError::fatal)
    }

    fn recoverable(self) -> Self::Target {
        self.map_err(ToDataSourceError::recoverable)
    }
}

impl<E> ToDataSourceError for E
where
    E: Into<Error>,
{
    type Target = DataSourceError;

    fn fatal(self) -> Self::Target {
        DataSourceError::Fatal(self.into())
    }

    fn recoverable(self) -> Self::Target {
        DataSourceError::Recoverable(self.into())
    }
}

#[async_trait]
pub trait Source: std::fmt::Debug + Send {
    async fn merge_to_figment(&self, figment: Figment) -> Result<Figment>;

    async fn watch(&mut self, _notify: Notifier) {}
}

pub struct SourceRegistry {
    pub sources: Vec<Box<dyn Source + Sync + Send>>,
    watch_tx: mpsc::Sender<()>,
    watch_rx: Option<mpsc::Receiver<()>>,
}

impl SourceRegistry {
    pub fn new<I: Iterator<Item = Box<dyn Source + Sync + Send>>>(sources: I) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(1);
        let sources = sources.collect();
        Self {
            sources,
            watch_tx,
            watch_rx: Some(watch_rx),
        }
    }

    /// Watch for changes on the underlying data sources.
    ///
    /// # Panics
    /// panics if `watch` is called multiple times on a [SourceRegistry].
    pub async fn watch<
        'a,
        F: Fn(&'a SourceRegistry) -> Pin<Box<dyn futures::Future<Output = ()> + Send + 'a>>,
    >(
        &'a mut self,
        cb: F,
    ) {
        let Some(mut watch_rx) = self.watch_rx.take() else {
            panic!("This source registry is already being watched.");
        };

        for source in self.sources.iter_mut() {
            let notifier = Notifier::new(self.watch_tx.clone());
            log::debug!("watching source: {source:?}");
            source.watch(notifier).await
        }

        // Downgrade to shared reference here.
        let self_ = &*self;

        loop {
            let Some(()) = watch_rx.recv().await else {
                log::debug!("All watchers terminated.");
                break;
            };

            cb(self_).await;
        }
    }

    /// Extract the layered data sources into a [Figment].
    pub async fn as_figment(&self) -> crate::error::Result<Figment> {
        let mut figment = Figment::new();
        for source in &self.sources {
            log::debug!("Reading source: {source:?}");
            figment = match source.merge_to_figment(figment.clone()).await {
                Err(e) if e.is_recoverable() => {
                    log::warn!("Datasource {source:?} is not available: {e}");
                    continue;
                }
                other => other,
            }?;
        }
        Ok(figment)
    }
}

impl Debug for SourceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceRegistry")
            .field("sources", &self.sources)
            .finish()
    }
}

use std::pin::Pin;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::datasource::{Notifier, SourceRegistry};

pub struct WatcherRegistry<'a> {
    sources: &'a mut SourceRegistry,
    watch_tx: mpsc::Sender<()>,
    watch_rx: Option<mpsc::Receiver<()>>,
}

#[async_trait]
pub trait Watch: std::fmt::Debug + Send {
    async fn watch(&mut self, _notify: Notifier) {}
}

impl<'a> WatcherRegistry<'a> {
    pub fn new(sources: &'a mut SourceRegistry) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(1);
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
        'b,
        F: Fn(&'b SourceRegistry) -> Pin<Box<dyn futures::Future<Output = ()> + Send + 'b>>,
    >(
        &'b mut self,
        cb: F,
    ) {
        let Some(mut watch_rx) = self.watch_rx.take() else {
            panic!("This source registry is already being watched.");
        };

        for source in self.sources.sources.iter_mut() {
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

            cb(self_.sources).await;
        }
    }
}

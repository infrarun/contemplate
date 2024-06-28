use std::pin::Pin;

#[cfg(feature = "webhook")]
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::datasource::{Notifier, SourceRegistry};

pub struct WatcherRegistry<'a> {
    pub watchers: Vec<Box<dyn Watch + Sync + Send>>,
    sources: &'a mut SourceRegistry,
    watch_tx: mpsc::Sender<()>,
    watch_rx: Option<mpsc::Receiver<()>>,
}

#[async_trait]
pub trait Watch: std::fmt::Debug + Send {
    async fn watch(&mut self, _notify: Notifier) {}
}

impl<'a> WatcherRegistry<'a> {
    pub fn new<I: Iterator<Item = Box<dyn Watch + Sync + Send>>>(
        sources: &'a mut SourceRegistry,
        watchers: I,
    ) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(1);
        let watchers = watchers.collect();
        Self {
            sources,
            watchers,
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

        for watcher in self.watchers.iter_mut() {
            let notifier = Notifier::new(self.watch_tx.clone());
            log::debug!("running watcher: {watcher:?}");
            watcher.watch(notifier).await
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

#[cfg(feature = "poll")]
#[derive(Debug)]
pub struct PollWatcher {
    interval: tokio::time::Duration,
}

#[cfg(feature = "poll")]
impl PollWatcher {
    pub fn new(interval: tokio::time::Duration) -> Self {
        Self { interval }
    }
}

#[cfg(feature = "poll")]
#[async_trait]
impl Watch for PollWatcher {
    async fn watch(&mut self, notify: Notifier) {
        let self_dbg = format!("{:?}", *self);
        let mut interval = tokio::time::interval(self.interval);

        tokio::spawn(async move {
            loop {
                interval.tick().await;
                notify.notify_async(&self_dbg).await;
            }
        });
    }
}

#[cfg(feature = "webhook")]
#[derive(Debug)]
pub struct WebHook {
    listen: String,
}

#[cfg(feature = "webhook")]
impl WebHook {
    pub fn new(listen: String) -> Self {
        Self { listen }
    }
}

#[cfg(feature = "webhook")]
#[async_trait]
impl Watch for WebHook {
    async fn watch(&mut self, notify: Notifier) {
        let self_dbg = format!("{:?}", *self);
        let listener = tokio::net::TcpListener::bind(&self.listen).await.unwrap();
        let app = axum::Router::new()
            .route(
                "/",
                axum::routing::any(
                    |axum::extract::State((state, self_dbg)): axum::extract::State<(
                        Arc<Notifier>,
                        String,
                    )>| {
                        async move {
                            state.notify_async(&self_dbg).await;
                            "OK"
                        }
                    },
                ),
            )
            .with_state((Arc::new(notify), self_dbg));
        tokio::spawn(async { axum::serve(listener, app).await });
    }
}

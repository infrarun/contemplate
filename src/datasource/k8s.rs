use std::collections::BTreeMap;

use async_trait::async_trait;
use figment::{
    util::{map, nest},
    value::{Dict, Value},
    Metadata, Profile, Provider,
};
use futures::StreamExt;
use k8s_openapi::{api::core::v1, ByteString};
use kube::{
    runtime::{watcher, WatchStreamExt},
    Api, Client,
};

use crate::error::Error;

use super::{Notifier, Source, ToDataSourceError};

use coalesce::Coalescible;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ConfigMap {
    name: String,
    namespace: Option<String>,
}

impl ConfigMap {
    pub fn new<N: AsRef<str>, S: AsRef<str>>(name: N, namespace: Option<S>) -> Self {
        let name = name.as_ref().to_owned();
        let namespace = namespace.map(|s| s.as_ref().to_owned());
        Self { name, namespace }
    }
}

#[async_trait]
impl Source for ConfigMap {
    async fn merge_to_figment(&self, figment: figment::Figment) -> super::Result<figment::Figment> {
        let client: Client = Client::try_default().await.recoverable()?;

        let api: Api<v1::ConfigMap> = match self.namespace {
            Some(ref ns) => Api::namespaced(client, ns),
            None => Api::default_namespaced(client),
        };

        let Some(cm) = api.get_opt(&self.name).await.recoverable()? else {
            return Err(Error::ConfigMapDoesNotExist(self.name.clone())).recoverable();
        };

        let data: ConfigMapProvider = cm
            .data
            .ok_or_else(|| Error::ConfigMapDoesNotExist(self.name.clone()))
            .recoverable()?
            .into();
        let figment = figment.merge(data);
        Ok(figment)
    }

    async fn watch(&mut self, notify: Notifier) {
        let Ok(client) = Client::try_default().await.inspect_err(|e| {
            log::error!("Could not get k8s client: {e}");
        }) else {
            return;
        };

        let api: Api<v1::ConfigMap> = match self.namespace {
            Some(ref ns) => Api::namespaced(client, ns),
            None => Api::default_namespaced(client),
        };

        let config: watcher::Config =
            watcher::Config::default().fields(&format!("metadata.name={}", self.name));

        let self_dbg = format!("{:?}", *self);
        tokio::spawn(async move {
            watcher(api, config)
                .default_backoff()
                .applied_objects()
                .predicate_filter(kube::runtime::predicates::generation)
                .for_each(|event| async {
                    if let Err(e) = event {
                        log::warn!("K8s watcher error: {e}");
                        return;
                    }

                    notify.notify_async(&self_dbg).await;
                })
                .await;
        });
    }
}

struct ConfigMapProvider {
    data: BTreeMap<String, String>,
}

impl From<BTreeMap<String, String>> for ConfigMapProvider {
    fn from(value: BTreeMap<String, String>) -> Self {
        Self { data: value }
    }
}

impl Provider for ConfigMapProvider {
    fn metadata(&self) -> figment::Metadata {
        Metadata::named("k8s configmap").interpolater(move |_: &Profile, k: &[&str]| {
            let keys: Vec<_> = k.iter().map(|k| k.to_ascii_uppercase()).collect();

            keys.join(".")
        })
    }

    fn data(
        &self,
    ) -> Result<figment::value::Map<figment::Profile, figment::value::Dict>, figment::Error> {
        let mut dict = Dict::new();
        for (k, v) in self.data.iter().map(|(k, v)| {
            let key = k.to_ascii_lowercase().replace('_', ".");
            (key, v.to_owned())
        }) {
            let nested_dict: std::collections::BTreeMap<String, figment::value::Value> =
                nest(k.as_str(), v.parse().expect("infallible"))
                    .into_dict()
                    .expect("key is non-empty: must have dict");

            dict = dict.merge(nested_dict);
        }

        let profile = Profile::default();
        Ok(profile.collect(dict))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Secret {
    name: String,
    namespace: Option<String>,
}

impl Secret {
    pub fn new<N: AsRef<str>, S: AsRef<str>>(name: N, namespace: Option<S>) -> Self {
        let name = name.as_ref().to_owned();
        let namespace = namespace.map(|s| s.as_ref().to_owned());
        Self { name, namespace }
    }
}

#[async_trait]
impl Source for Secret {
    async fn merge_to_figment(&self, figment: figment::Figment) -> super::Result<figment::Figment> {
        let client: Client = Client::try_default().await.recoverable()?;

        let api: Api<v1::Secret> = match self.namespace {
            Some(ref ns) => Api::namespaced(client, ns),
            None => Api::default_namespaced(client),
        };

        let Some(secret) = api.get_opt(&self.name).await.recoverable()? else {
            return Err(Error::SecretDoesNotExist(self.name.clone())).recoverable();
        };

        let data: SecretProvider = secret
            .data
            .ok_or_else(|| Error::SecretDoesNotExist(self.name.clone()))
            .recoverable()?
            .into();
        let figment = figment.merge(data);
        Ok(figment)
    }

    async fn watch(&mut self, notify: Notifier) {
        let Ok(client) = Client::try_default().await.inspect_err(|e| {
            log::error!("Could not get k8s client: {e}");
        }) else {
            return;
        };

        let api: Api<v1::Secret> = match self.namespace {
            Some(ref ns) => Api::namespaced(client, ns),
            None => Api::default_namespaced(client),
        };

        let config: watcher::Config =
            watcher::Config::default().fields(&format!("metadata.name={}", self.name));

        let self_dbg = format!("{:?}", *self);
        tokio::spawn(async move {
            watcher(api, config)
                .default_backoff()
                .applied_objects()
                .predicate_filter(kube::runtime::predicates::generation)
                .for_each(|event| async {
                    if let Err(e) = event {
                        log::warn!("K8s watcher error: {e}");
                        return;
                    }

                    notify.notify_async(&self_dbg).await;
                })
                .await;
        });
    }
}

struct SecretProvider {
    data: BTreeMap<String, ByteString>,
}

impl From<BTreeMap<String, ByteString>> for SecretProvider {
    fn from(value: BTreeMap<String, ByteString>) -> Self {
        Self { data: value }
    }
}

impl Provider for SecretProvider {
    fn metadata(&self) -> figment::Metadata {
        Metadata::named("k8s secret").interpolater(move |_: &Profile, k: &[&str]| {
            let keys: Vec<_> = k.iter().map(|k| k.to_ascii_uppercase()).collect();

            keys.join(".")
        })
    }

    fn data(
        &self,
    ) -> Result<figment::value::Map<figment::Profile, figment::value::Dict>, figment::Error> {
        let mut dict = Dict::new();
        for (k, v) in self.data.iter().map(|(k, v)| {
            let key = k.to_ascii_lowercase().replace('_', ".");
            (key, v.to_owned())
        }) {
            let utf8 = String::from_utf8(v.0.clone()).ok().map(Value::from);
            let bytes = Value::from(v.0.as_slice());

            let value: Value = match utf8 {
                None => map!("bytes" => bytes),
                Some(utf8) => map!("bytes" => bytes, "string" => utf8),
            }
            .into();

            let nested_dict: std::collections::BTreeMap<String, figment::value::Value> =
                nest(k.as_str(), value)
                    .into_dict()
                    .expect("key is non-empty: must have dict");

            dict = dict.merge(nested_dict);
        }

        let profile = Profile::default();
        Ok(profile.collect(dict))
    }
}

// The following code is taken from figment, because it's not exposed there.
// see: https://github.com/SergioBenitez/Figment/issues/74
mod coalesce {
    use figment::value::{Map, Value};
    use figment::Profile;

    #[derive(Debug, Clone, Copy, PartialEq)]
    #[allow(dead_code)]
    pub enum Order {
        Merge,
        Join,
        Adjoin,
        Admerge,
    }

    pub trait Coalescible: Sized {
        fn coalesce(self, other: Self, order: Order) -> Self;
        fn merge(self, other: Self) -> Self {
            self.coalesce(other, Order::Merge)
        }
    }

    impl Coalescible for Profile {
        fn coalesce(self, other: Self, order: Order) -> Self {
            match order {
                Order::Join | Order::Adjoin => self,
                Order::Merge | Order::Admerge => other,
            }
        }
    }

    impl Coalescible for Value {
        fn coalesce(self, other: Self, o: Order) -> Self {
            use {Order::*, Value::Array as A, Value::Dict as D};
            match (self, other, o) {
                (D(t, a), D(_, b), Join | Adjoin) | (D(_, a), D(t, b), Merge | Admerge) => {
                    D(t, a.coalesce(b, o))
                }
                (A(t, mut a), A(_, b), Adjoin | Admerge) => A(t, {
                    a.extend(b);
                    a
                }),
                (v, _, Join | Adjoin) | (_, v, Merge | Admerge) => v,
            }
        }
    }

    impl<K: Eq + std::hash::Hash + Ord, V: Coalescible> Coalescible for Map<K, V> {
        fn coalesce(self, mut other: Self, order: Order) -> Self {
            let mut joined = Map::new();
            for (a_key, a_val) in self {
                match other.remove(&a_key) {
                    Some(b_val) => joined.insert(a_key, a_val.coalesce(b_val, order)),
                    None => joined.insert(a_key, a_val),
                };
            }

            // `b` contains `b - a`, i.e, additions. keep them all.
            joined.extend(other);
            joined
        }
    }
}

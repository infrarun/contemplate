use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use minijinja::{
    Environment, State, Value,
    value::{Enumerator, Object, ValueKind},
};
use serde::Serialize;
use tokio::sync::{RwLock, RwLockMappedWriteGuard, RwLockReadGuard, RwLockWriteGuard};

/// Utility object to hold a reference to the runtime and the context.
#[derive(Debug)]
pub struct ContextWithRuntime {
    rt: tokio::runtime::Handle,
    ctx: minijinja::Value,
    data: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl ContextWithRuntime {
    pub fn put_blocking<T>(&self, t: T)
    where
        T: Send + Sync + 'static,
    {
        self.data
            .blocking_write()
            .insert(TypeId::of::<T>(), Box::new(t));
    }

    pub async fn put<T>(&self, t: T)
    where
        T: Send + Sync + 'static,
    {
        self.data
            .write()
            .await
            .insert(TypeId::of::<T>(), Box::new(t));
    }

    pub async fn has<T: 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.data.read().await.contains_key(&type_id)
    }

    pub fn has_blocking<T: 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.data.blocking_read().contains_key(&type_id)
    }

    pub async fn get_ref<T: 'static>(&self) -> Option<RwLockReadGuard<'_, T>> {
        let type_id = TypeId::of::<T>();
        RwLockReadGuard::try_map(self.data.read().await, |data| {
            data.get(&type_id).and_then(|b| b.downcast_ref::<T>())
        })
        .ok()
    }

    pub async fn get_mut<T: 'static>(&self) -> Option<RwLockMappedWriteGuard<'_, T>> {
        let type_id = TypeId::of::<T>();
        RwLockWriteGuard::try_map(self.data.write().await, |data| {
            data.get_mut(&type_id).and_then(|b| b.downcast_mut::<T>())
        })
        .ok()
    }
}

impl Object for ContextWithRuntime {
    fn get_value(self: &Arc<Self>, name: &Value) -> Option<Value> {
        // `$context` is not a valid identifier, and thus cannot be resolved by templates.
        // Functions passed the [State](minijinja::State) can
        // pluck out a reference to the context using this keyword.
        if name.as_str() == Some("$context") {
            return Some(Value::from_dyn_object(self.clone()));
        }
        self.ctx.get_item(name).ok().filter(|x| !x.is_undefined())
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        if self.ctx.kind() == ValueKind::Map
            && let Ok(keys) = self.ctx.try_iter()
        {
            return Enumerator::Values(keys.collect());
        }
        Enumerator::Empty
    }
}

/// Given a context, wraps it so that the runtime is included.
pub fn capture_runtime_handle<S: Serialize>(ctx: S) -> Value {
    Value::from_object(ContextWithRuntime {
        ctx: Value::from_serialize(ctx),
        rt: tokio::runtime::Handle::current(),
        data: RwLock::new(HashMap::new()),
    })
}

/// Utility function to retrieve the current runtime handle from the template state.
pub fn get_runtime_handle(state: &State) -> tokio::runtime::Handle {
    state
        .lookup("$context")
        .expect("$context not passed")
        .downcast_object_ref::<ContextWithRuntime>()
        .expect("Could not downcast context")
        .rt
        .clone()
}

#[cfg(feature = "http")]
mod http;

pub fn register(#[allow(unused_variables)] env: &mut Environment) {
    #[cfg(feature = "http")]
    env.add_function("http", http::http_request);
}

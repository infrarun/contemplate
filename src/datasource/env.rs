use super::Source;
use crate::datasource::Result;
use async_trait::async_trait;
use figment::{providers::Env, Figment};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Environment {
    prefix: Option<String>,
}

impl Environment {
    pub fn new<S: AsRef<str>>(prefix: Option<S>) -> Self {
        let prefix = prefix.map(|s| s.as_ref().to_owned());
        Self { prefix }
    }
}

#[async_trait]
impl Source for Environment {
    async fn merge_to_figment(&self, figment: Figment) -> Result<Figment> {
        let env = match self.prefix {
            Some(ref prefix) => Env::prefixed(&format!("{prefix}_")),
            None => Env::raw(),
        };
        Ok(figment.merge(env.split("_")))
    }
}

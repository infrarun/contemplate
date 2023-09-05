use std::path::PathBuf;

use thiserror::Error;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O Error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Unknown file extension: {ext}")]
    UnknownFileExtension { ext: String },

    #[error("Unknown file type: {path:?}")]
    UnknownFileType { path: PathBuf },

    #[error("Cowardly refusing to overwrite the existing backup at {0:?}")]
    BackupWouldBeOverwritten(PathBuf),

    #[error("Figment error: {0}")]
    FigmentError(#[from] figment::Error),

    #[error("Argument error: {0}")]
    ClapError(#[from] clap::error::Error),

    #[error("Templating error: {0:#}")]
    TemplateError(#[from] minijinja::Error),

    #[error("System error: {0}")]
    NixError(#[from] nix::errno::Errno),

    #[error("Invalid signal argument")]
    CliInvalidSignal,

    #[error("K8s Error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("K8s configmap does not exist: {0}")]
    ConfigMapDoesNotExist(String),

    #[error("K8s secret does not exist: {0}")]
    SecretDoesNotExist(String),
}

pub type Result<T> = std::result::Result<T, Error>;

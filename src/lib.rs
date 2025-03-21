#[cfg(feature = "discover")]
pub mod discover;
pub mod ops;
pub mod readable;
pub mod retrieve;

#[derive(Debug, thiserror::Error)]
pub enum HcOpsError {
    #[error("Holochain client error: {0:?}")]
    HolochainClient(holochain_client::ConductorApiError),

    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] diesel::result::Error),

    #[error("JSON error: {0}")]
    JSON(#[from] serde_json::Error),

    #[error("HoloHash error: {0}")]
    HoloHash(#[from] holochain_zome_types::prelude::HoloHashError),

    #[error("Serialized bytes error: {0}")]
    SerializedBytes(#[from] holochain_serialized_bytes::SerializedBytesError),

    #[error("Other error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[cfg(feature = "discover")]
    #[error("Process lookup error: {0}")]
    ProcCtl(#[from] proc_ctl::ProcCtlError),

    #[error("{context}\n\tcaused by: {source}")]
    Context {
        #[source]
        source: Box<HcOpsError>,
        context: String,
    },
}

impl HcOpsError {
    pub fn client(error: holochain_client::ConductorApiError) -> Self {
        HcOpsError::HolochainClient(error)
    }

    pub fn other<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        HcOpsError::Other(Box::new(error))
    }
}

pub type HcOpsResult<T> = Result<T, HcOpsError>;

pub trait HcOpsResultContextExt<T> {
    fn context(self, context: impl Into<String>) -> HcOpsResult<T>;
}

impl<S> HcOpsResultContextExt<S> for HcOpsResult<S> {
    fn context(self, context: impl Into<String>) -> HcOpsResult<S> {
        self.map_err(|e| HcOpsError::Context {
            source: Box::new(e),
            context: context.into(),
        })
    }
}

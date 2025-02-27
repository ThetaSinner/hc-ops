#[cfg(feature = "discover")]
pub mod discover;
pub mod ops;

#[derive(Debug, thiserror::Error)]
pub enum HcOpsError {
    #[error("Holochain client error: {0:?}")]
    HolochainClient(holochain_client::ConductorApiError),

    #[error("Other error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[cfg(feature = "discover")]
    #[error("Process lookup error: {0}")]
    ProcCtl(#[from] proc_ctl::ProcCtlError),
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

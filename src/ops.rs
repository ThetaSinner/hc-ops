use crate::{HcOpsError, HcOpsResult};
use futures::FutureExt;
use futures::future::BoxFuture;
use holochain_client::{AgentSigner, InstalledAppId};
use holochain_conductor_api::IssueAppAuthenticationTokenPayload;
use holochain_zome_types::prelude::CellId;
use std::net::IpAddr;
use std::sync::Arc;

pub trait AdminWebsocketExt {
    /// Check whether a running cell has been initialized.
    fn is_cell_initialized(&self, cell_id: CellId) -> BoxFuture<'static, HcOpsResult<bool>>;

    /// Discover or create an app interface, then connect to it.
    ///
    /// Inputs;
    /// - `addr`: Address to connect to, which should be the same as you used to connect this
    ///   admin client.
    /// - `app_id`: Installed app id of the app you want to connect to.
    /// - `origin`: The origin header to use in the request. This should identify your application.
    /// - `signer`: The agent signer to use for signing zome calls.
    fn connect_app_client(
        &self,
        addr: IpAddr,
        app_id: InstalledAppId,
        origin: impl ToString,
        signer: Arc<dyn AgentSigner + Send + Sync>,
    ) -> BoxFuture<'static, HcOpsResult<holochain_client::AppWebsocket>>;
}

impl AdminWebsocketExt for holochain_client::AdminWebsocket {
    fn is_cell_initialized(&self, cell_id: CellId) -> BoxFuture<'static, HcOpsResult<bool>> {
        let this = self.clone();
        async move {
            let state = this.dump_state(cell_id).await.map_err(HcOpsError::client)?;

            let dump: serde_json::Value =
                serde_json::from_str(&state).map_err(HcOpsError::other)?;

            let records = dump
                // Returns a tuple
                .as_array()
                // First value in the tuple is the JSON dump
                .and_then(|tuple| tuple.first())
                // The dump is a `JsonDump`
                .and_then(|first| first.as_object())
                // Should contain a `source_chain_dump` which is a `SourceChainDump`
                .and_then(|obj| obj.get("source_chain_dump").and_then(|v| v.as_object()))
                // Should contain a list of records
                .and_then(|v| v.get("records").and_then(|v| v.as_array()));

            match records {
                Some(records) => {
                    for record in records {
                        let typ = record
                            .get("action")
                            .and_then(|v| v.as_object())
                            .and_then(|v| v.get("type"))
                            .and_then(|v| v.as_str());

                        if typ == Some("InitZomesComplete") {
                            return Ok(true);
                        }
                    }
                }
                None => {
                    return Err(HcOpsError::Other("No records found in dump".into()));
                }
            }

            Ok(false)
        }
        .boxed()
    }

    fn connect_app_client(
        &self,
        addr: IpAddr,
        app_id: InstalledAppId,
        origin: impl ToString,
        signer: Arc<dyn AgentSigner + Send + Sync>,
    ) -> BoxFuture<'static, HcOpsResult<holochain_client::AppWebsocket>> {
        let this = self.clone();
        let origin = origin.to_string();
        async move {
            let app_interfaces = this
                .list_app_interfaces()
                .await
                .map_err(HcOpsError::client)?;

            let selected_interface = app_interfaces.iter().find(|i| {
                // If the interface is dedicated to some other app, we can't use it
                match i.installed_app_id {
                    Some(ref id) if id.as_ref() != app_id => {
                        return false;
                    }
                    _ => {}
                }

                if !i.allowed_origins.is_allowed(origin.as_ref()) {
                    return false;
                }

                true
            });

            let use_port = match selected_interface {
                Some(i) => i.port,
                None => this
                    .attach_app_interface(0, None, origin.to_string().into(), None)
                    .await
                    .map_err(HcOpsError::client)?,
            };

            let token_response = this
                .issue_app_auth_token(IssueAppAuthenticationTokenPayload::for_installed_app_id(
                    app_id,
                ))
                .await
                .map_err(HcOpsError::client)?;

            let app_client = holochain_client::AppWebsocket::connect(
                (addr, use_port),
                token_response.token,
                signer,
                Some(origin),
            )
            .await
            .map_err(|e| {
                HcOpsError::Other(format!("Could not connect app interface: {:?}", e).into())
            })?;

            Ok(app_client)
        }
        .boxed()
    }
}

//! Discover Holochain processes on the local machine.
//!
//! ```rust,no_run
//! use std::net::{Ipv4Addr, SocketAddr};
//! use hc_ops::discover::*;
//!
//! async fn discover() {
//!     let possible_processes = discover_possible_processes("holochain").unwrap();
//!
//!     if possible_processes.len() == 1 {
//!         let admin_addr = discover_admin_addr(&possible_processes[0].1).await.unwrap();
//!
//!         if let Some(addr) = admin_addr {
//!             // We found a Holochain process with an open admin port
//!             holochain_client::AdminWebsocket::connect(
//!                 addr,
//!             )
//!             .await
//!             .unwrap();
//!         } else {
//!             // Could be several reasons for this:
//!             // - The process is not a Holochain process
//!             // - The process is a Holochain process but the admin port is not open
//!             // - The process is a Holochain process but the version does not match the client we're using
//!         }
//!     } else {
//!         // Prompt the user or otherwise filter the list of possible processes
//!     }
//! }
//! ```

use crate::HcOpsResult;
use futures::FutureExt;
use holochain_client::WebsocketConfig;
use proc_ctl::{PortQuery, ProcInfo, ProcQuery, ProtocolPort};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

pub fn discover_possible_processes(
    process_name: impl AsRef<str>,
) -> HcOpsResult<Vec<(ProcInfo, Vec<u16>)>> {
    let query = ProcQuery::new().process_name(process_name.as_ref());
    let processes = query.list_processes()?;

    Ok(processes
        .into_iter()
        .filter_map(|p| {
            let port_query = PortQuery::new().ip_v4_only().tcp_only().process_id(p.pid);

            match port_query.execute() {
                Ok(ports) => {
                    let tcp_ports = ports
                        .into_iter()
                        .filter_map(|p| match p {
                            ProtocolPort::Tcp(p) => Some(p),
                            _ => None,
                        })
                        .collect::<Vec<_>>();

                    if tcp_ports.is_empty() {
                        None
                    } else {
                        Some((p, tcp_ports))
                    }
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>())
}

pub async fn discover_admin_addr(ports: &[u16]) -> HcOpsResult<Option<SocketAddr>> {
    for port in ports {
        if let Some(out) = test_admin_port(*port).await {
            return Ok(Some(out));
        }
    }

    Ok(None)
}

async fn test_admin_port(port: u16) -> Option<SocketAddr> {
    let ipv6_addr: SocketAddr = (Ipv6Addr::LOCALHOST, port).into();
    let ipv4_addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();

    let mut cfg = WebsocketConfig::CLIENT_DEFAULT;
    cfg.default_request_timeout = std::time::Duration::from_secs(1);
    let cfg = Arc::new(cfg);

    for addr in [ipv6_addr, ipv4_addr] {
        let req = holochain_websocket::ConnectRequest::new(addr)
            .try_set_header("Origin", "hc-ops")
            .unwrap();

        if let Ok((tx, mut rx)) = holochain_websocket::connect(cfg.clone(), req).await {
            let req = tx.request::<_, holochain_client::AdminResponse>(
                holochain_client::AdminRequest::ListApps {
                    status_filter: None,
                },
            );

            let (req_done_tx, mut req_done_rx) = futures::channel::oneshot::channel();
            let (recv_done_tx, mut recv_done_rx) = futures::channel::oneshot::channel();
            let (res_ok, recv_ok) = futures::join!(
                async move {
                    futures::select! {
                        _ = recv_done_rx => false,
                        res = req.fuse() => {
                            req_done_tx.send(()).ok();
                            res.is_ok()
                        }
                    }
                },
                async move {
                    loop {
                        futures::select! {
                            _ = req_done_rx => break,
                            res = rx.recv::<holochain_client::AdminResponse>().fuse() => {
                                if res.is_err() {
                                    recv_done_tx.send(false).ok();
                                    return false;
                                }
                            }
                        }
                    }

                    true
                }
            );

            if res_ok && recv_ok {
                return Some(addr);
            }
        }
    }

    None
}

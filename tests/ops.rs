use hc_ops::ops::AdminWebsocketExt;
use holochain::core::ActionHash;
use holochain::prelude::AppBundleSource;
use holochain::sweettest::SweetConductor;
use holochain_client::InstallAppPayload;
use holochain_conductor_api::CellInfo;
use std::net::Ipv4Addr;

#[tokio::test(flavor = "multi_thread")]
async fn check_app_init() {
    let conductor = SweetConductor::from_standard_config().await;

    conductor
        .clone()
        .install_app_bundle(InstallAppPayload {
            source: AppBundleSource::Path("fixture/happ/fixture.happ".into()),
            agent_key: None,
            installed_app_id: None,
            network_seed: None,
            roles_settings: None,
            ignore_genesis_failure: false,
        })
        .await
        .unwrap();

    conductor.enable_app("fixture".into()).await.unwrap();

    let running = conductor.list_apps(None).await.unwrap();
    let fixture_app = running
        .iter()
        .find(|app| app.installed_app_id == "fixture")
        .unwrap();

    let cell_info = fixture_app
        .cell_info
        .values()
        .next()
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .clone();

    let cell_id = match cell_info {
        CellInfo::Provisioned(pcell) => pcell.cell_id,
        _ => panic!("Cell not provisioned"),
    };

    let client = holochain_client::AdminWebsocket::connect(
        (
            Ipv4Addr::LOCALHOST,
            conductor.get_arbitrary_admin_websocket_port().unwrap(),
        ),
        None,
    )
    .await
    .unwrap();

    let initialized = client.is_cell_initialized(cell_id.clone()).await.unwrap();
    assert!(!initialized);

    conductor
        .easy_call_zome::<_, ActionHash, _>(
            cell_id.agent_pubkey(),
            None,
            cell_id.clone(),
            "fixture",
            "create",
            fixture_types::CreateTester {
                name: "Brandy".to_string(),
            },
        )
        .await
        .unwrap();

    let initialized = client.is_cell_initialized(cell_id).await.unwrap();
    assert!(initialized);
}

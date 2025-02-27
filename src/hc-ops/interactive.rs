#[cfg(feature = "discover")]
pub async fn interactive_discover_holochain_addr(
    name: String,
) -> anyhow::Result<std::net::SocketAddr> {
    use hc_ops::discover::{discover_admin_addr, discover_possible_processes};

    let mut possible = discover_possible_processes(name)?;

    // Ensure consistent ordering when multiple commands are run
    possible.sort_by_key(|(proc, _)| proc.pid);

    let (proc, ports) = if possible.is_empty() {
        anyhow::bail!("No Holochain processes found.");
    } else if possible.len() == 1 {
        possible.remove(0)
    } else {
        dialoguer::Select::new()
            .with_prompt("Pick a Holochain process")
            .default(0)
            .items(
                &possible
                    .iter()
                    .map(|(p, ports)| {
                        format!(
                            "Process ID: {}, launched with arguments: {:?}, has {} ports open",
                            p.pid,
                            p.cmd,
                            ports.len()
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .interact()
            .map(|idx| possible.remove(idx))
            .map_err(|e| anyhow::anyhow!(e))?
    };

    if let Some(addr) = discover_admin_addr(&ports).await? {
        Ok(addr)
    } else {
        anyhow::bail!("No admin ports found for process: {proc:?}.");
    }
}

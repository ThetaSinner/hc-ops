use crate::cli::{ConductorTagArgs, ConductorTagCommands};
use crate::data;
use crate::render::{ConductorTagTable, Render};
use diesel::SqliteConnection;
use std::net::SocketAddr;

pub(crate) async fn handle_conductor_tag_command(
    conn: &mut SqliteConnection,
    args: ConductorTagArgs,
) -> anyhow::Result<()> {
    match args.command {
        ConductorTagCommands::Add {
            tag,
            addr,
            port,
            #[cfg(feature = "discover")]
            name,
            #[cfg(feature = "discover")]
            origin,
        } => {
            if let (Some(addr), Some(port)) = (addr, port) {
                data::insert_conductor_tag(conn, &tag, SocketAddr::new(addr, port))?;
            } else {
                #[cfg(feature = "discover")]
                {
                    let addr =
                        crate::interactive::interactive_discover_holochain_addr(name, &origin)
                            .await?;
                    data::insert_conductor_tag(conn, &tag, addr)?;
                }
            }

            println!("Added tag: {}", tag);
        }
        ConductorTagCommands::List => {
            let tags = data::list_conductor_tags(conn)?;

            tags.into_iter()
                .map(Into::into)
                .collect::<Vec<ConductorTagTable>>()
                .render(std::io::stdout())?;
        }
        ConductorTagCommands::Delete { tag } => {
            data::delete_addr_tag(conn, &tag)?;

            println!("Deleted tag: {}", tag);
        }
    }

    Ok(())
}

use crate::cli::{TagArgs, TagCommands};
use crate::data;
use diesel::SqliteConnection;
use std::net::SocketAddr;

pub(crate) async fn handle_tag_command(
    conn: &mut SqliteConnection,
    args: TagArgs,
) -> anyhow::Result<()> {
    match args.command {
        TagCommands::Add {
            tag,
            addr,
            port,
            #[cfg(feature = "discover")]
            name,
        } => {
            if let (Some(addr), Some(port)) = (addr, port) {
                data::insert_addr_tag(conn, &tag, SocketAddr::new(addr, port))?;
            } else {
                #[cfg(feature = "discover")]
                {
                    let addr =
                        crate::interactive::interactive_discover_holochain_addr(name).await?;
                    data::insert_addr_tag(conn, &tag, addr)?;
                }
            }

            println!("Added tag: {}", tag);
        }
        TagCommands::List => {
            let tags = data::list_addr_tags(conn)?;

            for tag in tags {
                println!("{: <15}: ws://{}:{}", tag.tag, tag.address, tag.port);
            }
        }
        TagCommands::Delete { tag } => {
            data::delete_addr_tag(conn, &tag)?;

            println!("Deleted tag: {}", tag);
        }
    }

    Ok(())
}

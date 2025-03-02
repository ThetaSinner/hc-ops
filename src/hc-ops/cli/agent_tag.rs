use crate::cli::{AgentTagArgs, AgentTagCommands};
use crate::render::{AgentTagTable, Render};
use diesel::SqliteConnection;

pub(crate) async fn handle_agent_tag_command(
    conn: &mut SqliteConnection,
    args: AgentTagArgs,
) -> anyhow::Result<()> {
    match args.command {
        AgentTagCommands::Add { agent, tag } => {
            crate::data::insert_agent_tag(conn, &tag, agent.into())?;
            println!("Added tag: {}", tag);
        }
        AgentTagCommands::List => {
            let tags = crate::data::list_agent_tags(conn)?;

            if tags.is_empty() {
                println!("No tags found");
            } else {
                tags.into_iter()
                    .map(Into::into)
                    .collect::<Vec<AgentTagTable>>()
                    .render(std::io::stdout())?;
            }
        }
        AgentTagCommands::Delete { tag } => {
            crate::data::delete_agent_tag(conn, &tag)?;

            println!("Deleted tag: {}", tag);
        }
    }

    Ok(())
}

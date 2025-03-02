use crate::schema;
use anyhow::Context;
use diesel::prelude::*;
use holochain_client::AgentPubKey;

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::schema::addr_tag)]
#[diesel(primary_key(tag))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ConductorTag {
    pub tag: String,
    pub address: String,
    pub port: i32,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::addr_tag)]
pub struct NewConductorTag<'a> {
    pub tag: &'a str,
    pub address: &'a str,
    pub port: i32,
}

pub fn insert_conductor_tag(
    conn: &mut SqliteConnection,
    tag: &str,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    diesel::insert_into(schema::addr_tag::table)
        .values(&NewConductorTag {
            tag,
            address: addr.ip().to_string().as_str(),
            port: addr.port() as i32,
        })
        .execute(conn)
        .context("Is the tag already in use?")?;

    Ok(())
}

pub fn list_conductor_tags(conn: &mut SqliteConnection) -> anyhow::Result<Vec<ConductorTag>> {
    schema::addr_tag::table
        .order_by(schema::addr_tag::tag)
        .load(conn)
        .context("Failed to load conductor tags")
}

pub fn get_conductor_tag(
    conn: &mut SqliteConnection,
    tag: &str,
) -> anyhow::Result<Option<ConductorTag>> {
    schema::addr_tag::table
        .find(tag)
        .first(conn)
        .optional()
        .context("Failed to load conductor tag")
}

pub fn delete_addr_tag(conn: &mut SqliteConnection, tag: &str) -> anyhow::Result<()> {
    diesel::delete(schema::addr_tag::table.filter(schema::addr_tag::tag.eq(tag)))
        .execute(conn)
        .context("Failed to delete addr tag")?;

    Ok(())
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::schema::agent_tag)]
#[diesel(primary_key(agent))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AgentTag {
    pub agent: Vec<u8>,
    pub tag: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::agent_tag)]
pub struct NewAgentTag<'a> {
    pub tag: &'a str,
    pub agent: &'a [u8],
}

pub fn insert_agent_tag(
    conn: &mut SqliteConnection,
    tag: &str,
    agent: AgentPubKey,
) -> anyhow::Result<()> {
    diesel::insert_into(schema::agent_tag::table)
        .values(&NewAgentTag {
            tag,
            agent: agent.get_raw_39(),
        })
        .execute(conn)
        .context("Is the agent already tagged?")?;

    Ok(())
}

pub fn list_agent_tags(conn: &mut SqliteConnection) -> anyhow::Result<Vec<AgentTag>> {
    schema::agent_tag::table
        .order_by(schema::agent_tag::tag)
        .load(conn)
        .context("Failed to load agent tags")
}

pub fn get_agent_tag(
    conn: &mut SqliteConnection,
    agent: &AgentPubKey,
) -> anyhow::Result<Option<String>> {
    schema::agent_tag::table
        .find(agent.get_raw_39().to_vec())
        .first::<AgentTag>(conn)
        .optional()
        .map(|t| t.map(|t| t.tag))
        .context("Failed to load agent tag")
}

pub fn delete_agent_tag(conn: &mut SqliteConnection, tag: &str) -> anyhow::Result<()> {
    diesel::delete(schema::agent_tag::table.filter(schema::agent_tag::tag.eq(tag)))
        .execute(conn)
        .context("Failed to delete agent tag")?;

    Ok(())
}

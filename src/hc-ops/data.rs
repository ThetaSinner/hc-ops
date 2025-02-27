use crate::schema;
use anyhow::Context;
use diesel::prelude::*;

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::schema::addr_tag)]
#[diesel(primary_key(tag))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AddrTag {
    pub tag: String,
    pub address: String,
    pub port: i32,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::addr_tag)]
pub struct NewAddrTag<'a> {
    pub tag: &'a str,
    pub address: &'a str,
    pub port: i32,
}

pub fn insert_addr_tag(
    conn: &mut SqliteConnection,
    tag: &str,
    addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    diesel::insert_into(schema::addr_tag::table)
        .values(&NewAddrTag {
            tag,
            address: addr.ip().to_string().as_str(),
            port: addr.port() as i32,
        })
        .execute(conn)
        .context("Is the tag already in use?")?;

    Ok(())
}

pub fn list_addr_tags(conn: &mut SqliteConnection) -> anyhow::Result<Vec<AddrTag>> {
    schema::addr_tag::table
        .order_by(schema::addr_tag::tag)
        .load(conn)
        .context("Failed to load addr tags")
}

pub fn get_tag(conn: &mut SqliteConnection, tag: &str) -> anyhow::Result<Option<AddrTag>> {
    schema::addr_tag::table
        .find(tag)
        .first(conn)
        .optional()
        .context("Failed to load addr tag")
}

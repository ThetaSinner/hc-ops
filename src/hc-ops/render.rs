use holochain_zome_types::prelude::DnaHash;
use std::io;
use std::io::Write;
use tabled::settings::Style;
use tabled::{Table, Tabled};

fn flush(mut write: impl Write) -> io::Result<()> {
    write.write(b"\n")?;
    write.flush()?;
    Ok(())
}

#[derive(Tabled)]
pub struct InitStatus<'a> {
    pub app_id: &'a str,
    pub role: &'a str,
    pub dna_hash: &'a DnaHash,
    pub initialised: bool,
}

pub trait Render {
    fn render(&self, write: impl Write) -> io::Result<()>;
}

impl<Item> Render for Vec<Item>
where
    Item: Tabled,
{
    fn render(&self, mut write: impl Write) -> io::Result<()> {
        write.write(
            Table::new(self)
                .with(Style::modern_rounded())
                .to_string()
                .as_bytes(),
        )?;
        flush(write)
    }
}

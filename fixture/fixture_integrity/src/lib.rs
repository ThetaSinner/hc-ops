use hdi::prelude::*;

#[hdk_entry_helper]
pub struct Tester {
    pub name: String,
}

#[hdk_entry_types]
#[unit_enum(EntryTypesUnit)]
pub enum EntryTypes {
    Tester(Tester),
}

#[hdk_link_types]
pub enum LinkTypes {
    A,
    B,
}

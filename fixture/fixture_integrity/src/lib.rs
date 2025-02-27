use hdi::prelude::*;

#[hdk_entry_helper]
pub struct Tester;

#[hdk_entry_types]
#[unit_enum(EntryTypesUnit)]
pub enum EntryTypes {
    Tester(Tester),
}

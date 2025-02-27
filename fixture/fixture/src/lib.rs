use fixture_integrity::*;
use hdk::prelude::*;

#[hdk_extern]
fn create() -> ExternResult<ActionHash> {
    create_entry(EntryTypes::Tester(Tester))
}

use fixture_integrity::*;
use fixture_types::CreateTester;
use hdk::prelude::*;

#[hdk_extern]
fn init() -> ExternResult<InitCallbackResult> {
    put_sample_data(())?;

    Ok(InitCallbackResult::Pass)
}

#[hdk_extern]
fn create(input: CreateTester) -> ExternResult<ActionHash> {
    create_entry(EntryTypes::Tester(Tester { name: input.name }))
}

#[hdk_extern]
fn put_sample_data() -> ExternResult<()> {
    let mut sample_functions = HashSet::new();
    sample_functions.insert(("fixture".into(), "receive_remote_call".into()));
    create_cap_grant(CapGrantEntry {
        tag: "sample-unrestricted".into(),
        access: CapAccess::Unrestricted,
        functions: GrantedFunctions::Listed(sample_functions.clone()),
    })?;
    create_cap_grant(CapGrantEntry {
        tag: "sample-transferable".into(),
        access: CapAccess::Transferable {
            secret: generate_cap_secret()?,
        },
        functions: GrantedFunctions::Listed(sample_functions.clone()),
    })?;
    let mut assignees = BTreeSet::new();
    assignees.insert(AgentPubKey::from_raw_36(vec![1; 36]));
    create_cap_grant(CapGrantEntry {
        tag: "sample-assigned".into(),
        access: CapAccess::Assigned {
            secret: generate_cap_secret()?,
            assignees,
        },
        functions: GrantedFunctions::All,
    })?;

    create_cap_claim(CapClaimEntry {
        tag: "sample".to_string(),
        grantor: agent_info()?.agent_initial_pubkey,
        secret: generate_cap_secret()?,
    })?;

    let created = create(CreateTester {
        name: "Wednesday".into(),
    })?;
    update_entry(
        created.clone(),
        &EntryTypes::Tester(Tester {
            name: "Thursday".into(),
        }),
    )?;
    let deleted = delete_entry(DeleteInput {
        deletes_action_hash: created.clone(),
        chain_top_ordering: Default::default(),
    })?;

    let created_link = create_link(created.clone(), deleted.clone(), LinkTypes::A, ())?;
    delete_link(created_link, GetOptions::default())?;

    create_link(created, deleted, LinkTypes::B, "a tag")?;

    Ok(())
}

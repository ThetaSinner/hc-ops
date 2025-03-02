#![allow(non_snake_case)]

diesel::table! {
    DhtOp (hash) {
        hash -> Blob,
        #[sql_name = "type"]
        typ -> Nullable<Text>,
        basis_hash -> Nullable<Blob>,
        action_hash -> Nullable<Blob>,
        require_receipt -> Nullable<Bool>,
        storage_center_loc -> Nullable<Int4>,
        authored_timestamp -> Nullable<Int8>,
        op_order -> Text,
        validation_status -> Nullable<Int2>,
        when_integrated -> Nullable<Int8>,
        withhold_publish -> Nullable<Bool>,
        receipts_complete -> Nullable<Bool>,
        last_publish_time -> Nullable<Int8>,
        validation_stage -> Nullable<Int2>,
        num_validation_attempts -> Nullable<Int4>,
        last_validation_attempt -> Nullable<Int8>,
        dependency -> Nullable<Blob>,
    }
}

diesel::table! {
    Entry (hash) {
        hash -> Blob,
        blob -> Blob,
        tag -> Nullable<Text>,
        grantor -> Nullable<Blob>,
        cap_secret -> Nullable<Blob>,
        functions -> Nullable<Blob>,
        access_type -> Nullable<Text>,
        access_secret -> Nullable<Blob>,
        access_assignees -> Nullable<Blob>,
    }
}

diesel::table! {
    Action (hash) {
        hash -> Blob,
        #[sql_name = "type"]
        typ -> Text,
        seq -> Int4,
        author -> Blob,
        blob -> Blob,
        prev_hash -> Nullable<Blob>,
        entry_hash -> Nullable<Blob>,
        entry_type -> Nullable<Text>,
        private_entry -> Nullable<Bool>,
        original_entry_hash -> Nullable<Blob>,
        original_action_hash -> Nullable<Blob>,
        deletes_entry_hash -> Nullable<Blob>,
        deletes_action_hash -> Nullable<Blob>,
        base_hash -> Nullable<Blob>,
        zome_index -> Nullable<Int4>,
        link_type -> Nullable<Int4>,
        tag -> Nullable<Blob>,
        create_link_hash -> Nullable<Blob>,
        membrane_proof -> Nullable<Blob>,
        prev_dna_hash -> Nullable<Blob>,
    }
}

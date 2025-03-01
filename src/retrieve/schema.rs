#![allow(non_snake_case)]

diesel::table! {
    DhtOp (hash) {
        hash -> Blob,
        #[sql_name = "type"]
        typ -> Nullable<Text>,
        basis_hash -> Nullable<Blob>,
        action_hash -> Nullable<Blob>,
        require_receipt -> Nullable<Int2>,
        storage_center_loc -> Nullable<Int4>,
        authored_timestamp -> Nullable<Int8>,
    }
}

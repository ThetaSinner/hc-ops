// @generated automatically by Diesel CLI.

diesel::table! {
    addr_tag (tag) {
        tag -> Text,
        address -> Text,
        port -> Integer,
    }
}

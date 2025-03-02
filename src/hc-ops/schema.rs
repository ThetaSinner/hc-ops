// @generated automatically by Diesel CLI.

diesel::table! {
    addr_tag (tag) {
        tag -> Text,
        address -> Text,
        port -> Integer,
    }
}

diesel::table! {
    agent_tag (agent) {
        agent -> Binary,
        tag -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(addr_tag, agent_tag,);

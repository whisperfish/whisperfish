// @generated automatically by Diesel CLI.

diesel::table! {
    use diesel::sql_types::*;
    use crate::store::orm::IdentityMapping;

    identity_records (address) {
        address -> Text,
        record -> Binary,
        identity -> IdentityMapping,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::store::orm::IdentityMapping;

    kyber_prekeys (id) {
        id -> Integer,
        record -> Binary,
        identity -> IdentityMapping,
        is_last_resort -> Bool,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::store::orm::IdentityMapping;

    prekeys (id) {
        id -> Integer,
        record -> Binary,
        identity -> IdentityMapping,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::store::orm::IdentityMapping;

    sender_key_records (address, device, distribution_id) {
        address -> Text,
        device -> Integer,
        distribution_id -> Text,
        record -> Binary,
        created_at -> Timestamp,
        identity -> IdentityMapping,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::store::orm::IdentityMapping;
    session_records (address, device_id, identity) {
        address -> Text,
        device_id -> Integer,
        record -> Binary,
        identity -> IdentityMapping,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::store::orm::IdentityMapping;

    signed_prekeys (id) {
        id -> Integer,
        record -> Binary,
        identity -> IdentityMapping,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    identity_records,
    kyber_prekeys,
    prekeys,
    sender_key_records,
    session_records,
    signed_prekeys,
);

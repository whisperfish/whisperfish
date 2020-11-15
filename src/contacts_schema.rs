#![allow(non_snake_case)] // for phoneNumbers table and fields

table! {
    contacts (contactId) {
        contactId -> Integer,
        displayLabel -> Text,
    }
}

table! {
    phoneNumbers (id) {
        id -> Integer,
        phoneNumber -> Text,
        contactId -> Integer,
    }
}

joinable!(phoneNumbers -> contacts (contactId));
allow_tables_to_appear_in_same_query!(contacts, phoneNumbers,);

// @generated automatically by Diesel CLI.
pub mod migrations;
pub mod protocol;
pub use protocol::*;

diesel::table! {
    attachments (id) {
        id -> Integer,
        json -> Nullable<Text>,
        message_id -> Integer,
        content_type -> Text,
        name -> Nullable<Text>,
        content_disposition -> Nullable<Text>,
        content_location -> Nullable<Text>,
        attachment_path -> Nullable<Text>,
        is_pending_upload -> Bool,
        transfer_file_path -> Nullable<Text>,
        size -> Nullable<Integer>,
        file_name -> Nullable<Text>,
        unique_id -> Nullable<Text>,
        digest -> Nullable<Text>,
        is_voice_note -> Bool,
        is_borderless -> Bool,
        is_quote -> Bool,
        width -> Nullable<Integer>,
        height -> Nullable<Integer>,
        sticker_pack_id -> Nullable<Text>,
        sticker_pack_key -> Nullable<Binary>,
        sticker_id -> Nullable<Integer>,
        sticker_emoji -> Nullable<Text>,
        data_hash -> Nullable<Binary>,
        visual_hash -> Nullable<Text>,
        transform_properties -> Nullable<Text>,
        transfer_file -> Nullable<Text>,
        display_order -> Integer,
        upload_timestamp -> Timestamp,
        cdn_number -> Nullable<Integer>,
        caption -> Nullable<Text>,
        pointer -> Nullable<Binary>,
        transcription -> Nullable<Text>,
    }
}

diesel::table! {
    distribution_list_members (distribution_id, session_id) {
        distribution_id -> Text,
        session_id -> Integer,
        privacy_mode -> Integer,
    }
}

diesel::table! {
    distribution_lists (distribution_id) {
        name -> Text,
        distribution_id -> Text,
        session_id -> Nullable<Integer>,
        allows_replies -> Bool,
        deletion_timestamp -> Nullable<Timestamp>,
        is_unknown -> Bool,
        privacy_mode -> Integer,
    }
}

diesel::table! {
    group_v1_members (group_v1_id, recipient_id) {
        group_v1_id -> Text,
        recipient_id -> Integer,
        member_since -> Nullable<Timestamp>,
    }
}

diesel::table! {
    group_v1s (id) {
        id -> Text,
        name -> Text,
        expected_v2_id -> Nullable<Text>,
    }
}

diesel::table! {
    group_v2_members (group_v2_id, recipient_id) {
        group_v2_id -> Text,
        recipient_id -> Integer,
        member_since -> Timestamp,
        joined_at_revision -> Integer,
        role -> Integer,
    }
}

diesel::table! {
    group_v2s (id) {
        id -> Text,
        name -> Text,
        master_key -> Text,
        revision -> Integer,
        invite_link_password -> Nullable<Binary>,
        access_required_for_attributes -> Integer,
        access_required_for_members -> Integer,
        access_required_for_add_from_invite_link -> Integer,
        avatar -> Nullable<Text>,
        description -> Nullable<Text>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::store::orm::MessageTypeMapping;

    messages (id) {
        id -> Integer,
        session_id -> Integer,
        text -> Nullable<Text>,
        sender_recipient_id -> Nullable<Integer>,
        received_timestamp -> Nullable<Timestamp>,
        sent_timestamp -> Nullable<Timestamp>,
        server_timestamp -> Timestamp,
        is_read -> Bool,
        is_outbound -> Bool,
        flags -> Integer,
        expires_in -> Nullable<Integer>,
        expiry_started -> Nullable<Timestamp>,
        schedule_send_time -> Nullable<Timestamp>,
        is_bookmarked -> Bool,
        use_unidentified -> Bool,
        is_remote_deleted -> Bool,
        sending_has_failed -> Bool,
        quote_id -> Nullable<Integer>,
        story_type -> Integer,
        server_guid -> Nullable<Text>,
        message_ranges -> Nullable<Binary>,
        latest_revision_id -> Nullable<Integer>,
        original_message_id -> Nullable<Integer>,
        revision_number -> Integer,
        message_type -> Nullable<MessageTypeMapping>,
        expire_timer_version -> Integer,
    }
}

diesel::table! {
    reactions (reaction_id) {
        reaction_id -> Integer,
        message_id -> Integer,
        author -> Integer,
        emoji -> Text,
        sent_time -> Timestamp,
        received_time -> Timestamp,
    }
}

diesel::table! {
    receipts (message_id, recipient_id) {
        message_id -> Integer,
        recipient_id -> Integer,
        delivered -> Nullable<Timestamp>,
        read -> Nullable<Timestamp>,
        viewed -> Nullable<Timestamp>,
    }
}

diesel::table! {
    recipients (id) {
        id -> Integer,
        e164 -> Nullable<Text>,
        uuid -> Nullable<Text>,
        username -> Nullable<Text>,
        email -> Nullable<Text>,
        is_blocked -> Bool,
        profile_key -> Nullable<Binary>,
        profile_key_credential -> Nullable<Binary>,
        profile_given_name -> Nullable<Text>,
        profile_family_name -> Nullable<Text>,
        profile_joined_name -> Nullable<Text>,
        signal_profile_avatar -> Nullable<Text>,
        profile_sharing_enabled -> Bool,
        last_profile_fetch -> Nullable<Timestamp>,
        storage_service_id -> Nullable<Binary>,
        storage_proto -> Nullable<Binary>,
        capabilities -> Integer,
        last_gv1_migrate_reminder -> Nullable<Timestamp>,
        last_session_reset -> Nullable<Timestamp>,
        about -> Nullable<Text>,
        about_emoji -> Nullable<Text>,
        is_registered -> Bool,
        unidentified_access_mode -> Integer,
        pni -> Nullable<Text>,
        needs_pni_signature -> Bool,
        external_id -> Nullable<Text>,
    }
}

diesel::table! {
    sessions (id) {
        id -> Integer,
        direct_message_recipient_id -> Nullable<Integer>,
        group_v1_id -> Nullable<Text>,
        group_v2_id -> Nullable<Text>,
        is_archived -> Bool,
        is_pinned -> Bool,
        is_silent -> Bool,
        is_muted -> Bool,
        draft -> Nullable<Text>,
        expiring_message_timeout -> Nullable<Integer>,
    }
}

diesel::table! {
    settings (key) {
        key -> Text,
        value -> Text,
    }
}

diesel::table! {
    stickers (pack_id, sticker_id) {
        pack_id -> Nullable<Text>,
        sticker_id -> Integer,
        cover_sticker_id -> Integer,
        key -> Binary,
        title -> Text,
        author -> Text,
        pack_order -> Integer,
        emoji -> Text,
        content_type -> Nullable<Text>,
        last_used -> Timestamp,
        installed -> Timestamp,
        file_path -> Text,
        file_length -> Integer,
        file_random -> Binary,
    }
}

diesel::table! {
    story_sends (message_id, session_id) {
        message_id -> Integer,
        session_id -> Integer,
        sent_timestamp -> Timestamp,
        allows_replies -> Bool,
        distribution_id -> Text,
    }
}

diesel::joinable!(attachments -> messages (message_id));
diesel::joinable!(distribution_list_members -> distribution_lists (distribution_id));
diesel::joinable!(distribution_list_members -> sessions (session_id));
diesel::joinable!(distribution_lists -> sessions (session_id));
diesel::joinable!(group_v1_members -> group_v1s (group_v1_id));
diesel::joinable!(group_v1_members -> recipients (recipient_id));
diesel::joinable!(group_v2_members -> group_v2s (group_v2_id));
diesel::joinable!(group_v2_members -> recipients (recipient_id));
diesel::joinable!(messages -> recipients (sender_recipient_id));
diesel::joinable!(messages -> sessions (session_id));
diesel::joinable!(reactions -> messages (message_id));
diesel::joinable!(reactions -> recipients (author));
diesel::joinable!(receipts -> messages (message_id));
diesel::joinable!(receipts -> recipients (recipient_id));
diesel::joinable!(sessions -> group_v1s (group_v1_id));
diesel::joinable!(sessions -> group_v2s (group_v2_id));
diesel::joinable!(sessions -> recipients (direct_message_recipient_id));
diesel::joinable!(story_sends -> distribution_lists (distribution_id));
diesel::joinable!(story_sends -> messages (message_id));
diesel::joinable!(story_sends -> sessions (session_id));

diesel::allow_tables_to_appear_in_same_query!(
    attachments,
    distribution_list_members,
    distribution_lists,
    group_v1_members,
    group_v1s,
    group_v2_members,
    group_v2s,
    messages,
    reactions,
    receipts,
    recipients,
    sessions,
    settings,
    stickers,
    story_sends,
);

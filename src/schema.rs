table! {
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
    }
}

table! {
    group_v1_members (group_v1_id, recipient_id) {
        group_v1_id -> Text,
        recipient_id -> Integer,
        member_since -> Nullable<Timestamp>,
    }
}

table! {
    group_v1s (id) {
        id -> Text,
        name -> Text,
        expected_v2_id -> Nullable<Text>,
    }
}

table! {
    group_v2_members (group_v2_id, recipient_id) {
        group_v2_id -> Text,
        recipient_id -> Integer,
        member_since -> Timestamp,
        joined_at_revision -> Integer,
        role -> Integer,
    }
}

table! {
    group_v2s (id) {
        id -> Text,
        name -> Text,
        master_key -> Text,
        revision -> Integer,
        invite_link_password -> Nullable<Binary>,
        access_required_for_attributes -> Integer,
        access_required_for_members -> Integer,
        access_required_for_add_from_invite_link -> Integer,
    }
}

table! {
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
    }
}

table! {
    reactions (reaction_id) {
        reaction_id -> Integer,
        message_id -> Integer,
        author -> Integer,
        emoji -> Text,
        sent_time -> Timestamp,
        received_time -> Timestamp,
    }
}

table! {
    receipts (message_id, recipient_id) {
        message_id -> Integer,
        recipient_id -> Integer,
        delivered -> Nullable<Timestamp>,
        read -> Nullable<Timestamp>,
        viewed -> Nullable<Timestamp>,
    }
}

table! {
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
        unidentified_access_mode -> Bool,
        storage_service_id -> Nullable<Binary>,
        storage_proto -> Nullable<Binary>,
        capabilities -> Integer,
        last_gv1_migrate_reminder -> Nullable<Timestamp>,
        last_session_reset -> Nullable<Timestamp>,
    }
}

table! {
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

table! {
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

joinable!(attachments -> messages (message_id));
joinable!(group_v1_members -> recipients (recipient_id));
joinable!(group_v2_members -> group_v2s (group_v2_id));
joinable!(group_v2_members -> recipients (recipient_id));
joinable!(messages -> recipients (sender_recipient_id));
joinable!(messages -> sessions (session_id));
joinable!(reactions -> messages (message_id));
joinable!(reactions -> recipients (author));
joinable!(receipts -> messages (message_id));
joinable!(receipts -> recipients (recipient_id));
joinable!(sessions -> group_v1s (group_v1_id));
joinable!(sessions -> group_v2s (group_v2_id));
joinable!(sessions -> recipients (direct_message_recipient_id));

allow_tables_to_appear_in_same_query!(
    attachments,
    group_v1_members,
    group_v1s,
    group_v2_members,
    group_v2s,
    messages,
    reactions,
    receipts,
    recipients,
    sessions,
    stickers,
);

use crate::store::protos as database_protos;
use libsignal_service::proto::{body_range as wire_body_range, BodyRange as WireBodyRange};
use prost::Message;

#[tracing::instrument(level = "debug", name = "body_ranges::serialize")]
pub fn serialize(value: &[WireBodyRange]) -> Option<Vec<u8>> {
    if value.is_empty() {
        return None;
    }

    let message_ranges = database_protos::BodyRangeList {
        ranges: value
            .iter()
            .map(|range| {
                tracing::trace!(av = ?range.associated_value, start = range.start, len = range.length, "processing range");
                database_protos::body_range_list::BodyRange {
                    start: range.start.expect("start") as i32,
                    length: range.length.expect("end") as i32,
                    associated_value: range.associated_value.as_ref().map(|av| match av {
                        wire_body_range::AssociatedValue::MentionAci(mention_aci) => {
                            database_protos::body_range_list::body_range::AssociatedValue::MentionUuid(
                                mention_aci.clone(),
                            )
                        }
                        wire_body_range::AssociatedValue::Style(style) => {
                            database_protos::body_range_list::body_range::AssociatedValue::Style(*style)
                        }
                    }),
                }
            })
            .collect(),
    };

    Some(message_ranges.encode_to_vec())
}

#[tracing::instrument(level = "debug", skip(message_ranges), fields(message_ranges_len = message_ranges.map(Vec::len)), name="body_ranges::to_vec")]
pub fn to_vec(message_ranges: Option<&Vec<u8>>) -> Vec<WireBodyRange> {
    let Some(message_ranges) = message_ranges else {
        return vec![];
    };

    let message_ranges = database_protos::BodyRangeList::decode(message_ranges as &[u8])
        .expect("valid protobuf in database");
    message_ranges
        .ranges
        .iter()
        .flat_map(|range| {
            let associated_value = match range
                .associated_value
                .as_ref()
                .expect("associated_value in db")
            {
                database_protos::body_range_list::body_range::AssociatedValue::MentionUuid(
                    mention_aci,
                ) => wire_body_range::AssociatedValue::MentionAci(mention_aci.clone()),
                database_protos::body_range_list::body_range::AssociatedValue::Style(style) => {
                    wire_body_range::AssociatedValue::Style(*style)
                }
                database_protos::body_range_list::body_range::AssociatedValue::Link(link) => {
                    tracing::warn!("Not encoding link {link}");
                    return None;
                }
                database_protos::body_range_list::body_range::AssociatedValue::Button(button) => {
                    tracing::warn!("Not encoding button {button:?}");
                    return None;
                }
            };

            tracing::trace!(av = ?range.associated_value, start = range.start, len = range.length, "processed range");

            Some(WireBodyRange {
                start: Some(range.start as u32),
                length: Some(range.length as u32),
                associated_value: Some(associated_value),
            })
        })
        .collect()
}

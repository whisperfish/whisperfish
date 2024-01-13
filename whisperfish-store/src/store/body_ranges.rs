use crate::store::protos as database_protos;
use libsignal_service::proto::BodyRange as WireBodyRange;

pub fn serialize(value: &[WireBodyRange]) -> Option<Vec<u8>> {
    if value.is_empty() {
        return None;
    }

    todo!()
}

pub fn to_vec(message_ranges: Option<&Vec<u8>>) -> Vec<WireBodyRange> {
    let Some(message_ranges) = message_ranges else {
        return vec![];
    };

    todo!()
}

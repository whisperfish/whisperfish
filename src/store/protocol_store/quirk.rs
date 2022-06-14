//! Quirks for the on-disk structures of textsecure.
//!
//! Textsecure uses the same on-disk protobuf format as libsignal-protocol, however, some of the
//! byte-array fields have a quirky behaviour. This module provides methods to add and remove those
//! quirks.
//!
//! This module maps on <https://gitlab.com/whisperfish/whisperfish/-/issues/74>.

use libsignal_service::prelude::protocol::SignalProtocolError;
use prost::Message;

include!(concat!(env!("OUT_DIR"), "/textsecure.rs"));

pub const DJB_TYPE: u8 = 0x05;

fn prost_err_to_signal(e: prost::DecodeError) -> SignalProtocolError {
    SignalProtocolError::InvalidArgument(format!("Decoding in quirks: {}", e))
}

/// Removes quirks to the pre key data format that are apparent in Whisperfish 0.5
pub fn pre_key_from_0_5(input: &[u8]) -> Result<Vec<u8>, SignalProtocolError> {
    let mut obj = PreKeyRecordStructure::decode(input).map_err(prost_err_to_signal)?;

    // begin quirking
    unquirk_identity(&mut obj.public_key)?;
    // end quirking

    Ok(obj.encode_to_vec())
}

/// Adds quirks to the pre key data format that are apparent in Whisperfish 0.5
pub fn pre_key_to_0_5(input: &[u8]) -> Result<Vec<u8>, SignalProtocolError> {
    let mut obj = PreKeyRecordStructure::decode(input).map_err(prost_err_to_signal)?;

    // begin quirking
    quirk_identity(&mut obj.public_key)?;
    // end quirking

    Ok(obj.encode_to_vec())
}

/// Removes quirks to the signed pre key data format that are apparent in Whisperfish 0.5
pub fn signed_pre_key_from_0_5(input: &[u8]) -> Result<Vec<u8>, SignalProtocolError> {
    let mut obj = SignedPreKeyRecordStructure::decode(input).map_err(prost_err_to_signal)?;

    // begin quirking
    unquirk_identity(&mut obj.public_key)?;
    // end quirking

    Ok(obj.encode_to_vec())
}

/// Adds quirks to the signed pre key data format that are apparent in Whisperfish 0.5
pub fn signed_pre_key_to_0_5(input: &[u8]) -> Result<Vec<u8>, SignalProtocolError> {
    let mut obj = SignedPreKeyRecordStructure::decode(input).map_err(prost_err_to_signal)?;

    // begin quirking
    quirk_identity(&mut obj.public_key)?;
    // end quirking

    Ok(obj.encode_to_vec())
}

fn quirk_identity(id: &mut Vec<u8>) -> Result<(), SignalProtocolError> {
    if id.len() == 32 {
        log::warn!("Not quirking input key of 32 bytes!");
        Ok(())
    } else if id.len() == 32 + 1 {
        let removed = id.remove(0);
        if removed != DJB_TYPE {
            log::error!("Unknown input key type {}, not quirking.", removed);
            Err(SignalProtocolError::BadKeyType(removed))
        } else {
            Ok(())
        }
    } else {
        log::error!("Invalid input key of length {}", id.len());
        Err(SignalProtocolError::InvalidArgument(
            "Invalid identity key length".into(),
        ))
    }
}

fn unquirk_identity(id: &mut Vec<u8>) -> Result<(), SignalProtocolError> {
    if id.len() == 33 {
        log::warn!(
            "Not unquirking input key of 33 bytes! Its tarts with {}.",
            id[0]
        );
        Ok(())
    } else if id.len() == 32 {
        id.insert(0, DJB_TYPE);
        Ok(())
    } else {
        log::error!("Invalid input key of length {}, cannot unquirk", id.len());
        Err(SignalProtocolError::InvalidArgument(
            "Invalid identity key length".into(),
        ))
    }
}

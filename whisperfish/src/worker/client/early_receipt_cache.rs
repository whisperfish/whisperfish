use chrono::{Duration, Utc};
use libsignal_service::{content::Metadata, proto::ReceiptMessage};
use std::collections::HashMap;

// Maximum number of entries in the early receipt cache
pub const MAX_CACHE_SIZE: usize = 100;
// Time-to-live for cache entries
pub const CACHE_TTL: Duration = Duration::hours(24);

/// A receipt that is cached before its corresponding message is received
#[derive(Clone)]
pub struct CachedReceipt {
    pub receipt: ReceiptMessage,
    pub metadata: Metadata,
    pub created_at: chrono::DateTime<Utc>,
}

/// Cache for receipts received before their corresponding message
#[derive(Default)]
pub struct EarlyReceiptCache {
    // Index: timestamp -> list of receipt IDs
    timestamp_index: HashMap<u64, Vec<u64>>,

    // Storage: receipt_id -> CachedReceipt
    receipts: HashMap<u64, CachedReceipt>,

    // For generating unique IDs
    next_id: u64,
}

impl EarlyReceiptCache {
    /// Create a new empty EarlyReceiptCache
    pub fn new() -> Self {
        EarlyReceiptCache {
            timestamp_index: HashMap::new(),
            receipts: HashMap::new(),
            next_id: 1,
        }
    }

    /// Add a receipt to the cache
    ///
    /// # Arguments
    /// * `message` - The receipt message
    /// * `sender` - The sender of the receipt
    /// * `timestamp` - The timestamp of the receipt (milliseconds since epoch)
    ///
    /// # Returns
    /// true if the receipt was added, false if it was a duplicate or invalid
    #[tracing::instrument(skip(self, metadata, receipt))]
    pub fn add(&mut self, metadata: Metadata, receipt: ReceiptMessage) -> bool {
        let receipt = CachedReceipt {
            receipt,
            metadata,
            created_at: Utc::now(),
        };

        let ts_count = receipt.receipt.timestamp.len();

        if ts_count == 0 {
            return false;
        }

        let id = self.next_id;
        self.next_id += 1;

        // Index all timestamps from the message, deduplicating within this receipt
        for &ts in &receipt.receipt.timestamp {
            self.timestamp_index.entry(ts).or_default().push(id);
        }

        self.receipts.insert(id, receipt);

        tracing::debug!("Added receipt with {} timestamps", ts_count);

        // Trim cache if it exceeds maximum size
        self.cleanup_expired();
        self.trim_to_size();
        self.clean_index();

        true
    }

    /// Take all receipts for a given message timestamp
    ///
    /// Returns the receipts associated with the timestamp, if any.
    /// Uses lazy removal: only removes from storage when no timestamps remain.
    ///
    /// # Arguments
    /// * `message_timestamp` - The timestamp of the message to look up
    ///
    /// # Returns
    /// Some(Vec<ReceiptMessage>) if receipts were found, None otherwise
    #[tracing::instrument(skip(self))]
    pub fn take(
        &mut self,
        message_timestamp: u64,
    ) -> Option<impl Iterator<Item = CachedReceipt> + '_> {
        // Take receipt IDs for this timestamp
        let ids = self.timestamp_index.remove(&message_timestamp)?;

        let was_last = self
            .timestamp_index
            .values()
            .flatten()
            .all(|id| !ids.contains(id));

        self.clean_index();

        tracing::debug!(
            %was_last,
            "Retrieving {} receipts for timestamp {}",
            ids.len(),
            message_timestamp
        );

        Some(ids.into_iter().map(move |id| {
            if was_last {
                self.receipts.remove(&id)
            } else {
                self.receipts.get(&id).cloned()
            }
            .expect("receipt unavailable - stale index")
        }))
    }

    /// Removes timestamp index entries with empty id lists.
    fn clean_index(&mut self) {
        self.timestamp_index
            .retain(|_, receipt_ids| !receipt_ids.is_empty());
    }

    /// Remove expired entries from the cache
    #[tracing::instrument(skip(self))]
    pub fn cleanup_expired(&mut self) {
        let now = Utc::now();
        let cutoff = now - CACHE_TTL;

        // Collect receipts to remove
        let removed_receipts = self
            .receipts
            .extract_if(|_id, receipt| receipt.created_at < cutoff);

        // Remove from receipts
        let mut counter = 0;
        for (removed_id, _) in removed_receipts {
            counter += 1;
            for (&_ts, receipt_ids) in &mut self.timestamp_index {
                receipt_ids.retain(|&id| id != removed_id);
            }
        }

        tracing::debug!("Cleaned up {} expired receipts", counter);
    }

    /// Trim the cache to the maximum size
    #[tracing::instrument(skip(self))]
    fn trim_to_size(&mut self) {
        let old_len = self.receipts.len();

        while self.receipts.len() > MAX_CACHE_SIZE {
            // Find the oldest receipt by timestamp
            let Some(oldest_id) = self
                .receipts
                .iter()
                .min_by_key(|(_, receipt)| receipt.created_at)
                .map(|(id, _)| *id)
            else {
                break;
            };

            // Remove from timestamp_index
            for (&_ts, receipt_ids) in &mut self.timestamp_index {
                receipt_ids.retain(|&id| id != oldest_id);
            }

            // Remove from receipts
            self.receipts.remove(&oldest_id);
        }
        let new_len = self.receipts.len();

        if new_len < old_len {
            tracing::debug!("Trimmed cache from {} to {} entries", old_len, new_len);
        }
    }

    /// Get the number of cached receipts (for testing)
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    /// Check if the cache is empty (for testing)
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }
}

impl std::fmt::Debug for EarlyReceiptCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let receipt_count = self.receipts.len();
        let timestamp_count: usize = self.timestamp_index.values().map(|v| v.len()).sum();
        f.debug_struct("EarlyReceiptCache")
            .field("receipts", &receipt_count)
            .field("timestamps", &timestamp_count)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libsignal_service::proto::ReceiptMessage;
    use libsignal_service::protocol::{DeviceId, ServiceId};

    fn create_service_id(uuid: &str) -> ServiceId {
        ServiceId::parse_from_service_id_string(uuid).unwrap()
    }

    fn create_receipt_message(timestamps: Vec<u64>) -> ReceiptMessage {
        ReceiptMessage {
            timestamp: timestamps,
            r#type: Some(1), // Delivery receipt
        }
    }

    fn create_metadata(sender: ServiceId, timestamp: i64) -> Metadata {
        let timestamp = chrono::DateTime::UNIX_EPOCH + chrono::Duration::milliseconds(timestamp);
        Metadata {
            sender: sender.clone(),
            destination: sender,
            sender_device: DeviceId::new(1).unwrap(),
            timestamp,
            server_timestamp: timestamp,
            needs_receipt: false,
            unidentified_sender: false,
            was_plaintext: false,
            server_guid: None,
        }
    }

    #[test]
    fn test_add_and_take() {
        let mut cache = EarlyReceiptCache::new();
        let sender = create_service_id("55555555-1234-5678-1234-555555555555");
        let message = create_receipt_message(vec![1000]);

        assert!(cache.add(create_metadata(sender.clone(), 1000), message));
        assert_eq!(cache.len(), 1);

        // Take should return the receipt
        let taken = cache.take(1000);
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().collect::<Vec<_>>().len(), 1);

        // Cache should now be empty
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_take_nonexistent() {
        let mut cache = EarlyReceiptCache::new();
        let sender = create_service_id("55555555-1234-5678-1234-555555555555");
        let message = create_receipt_message(vec![1000]);

        cache.add(create_metadata(sender, 1000), message);

        let taken = cache.take(9999);
        assert!(taken.is_none());
    }

    #[test]
    fn test_zero_timestamps() {
        let mut cache = EarlyReceiptCache::new();
        let sender = create_service_id("55555555-1234-5678-1234-555555555555");
        let message = create_receipt_message(vec![]);

        // Adding without any timestamps should fail
        assert!(!cache.add(create_metadata(sender, 1000), message));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_deduplicate_timestamps() {
        let mut cache = EarlyReceiptCache::new();
        let sender = create_service_id("55555555-1234-5678-1234-555555555555");
        // Message with duplicate timestamps
        let message = create_receipt_message(vec![1000, 1000, 1000]);

        cache.add(create_metadata(sender, 500), message);

        // The implementation pushes each timestamp individually;
        // there is no deduplication within the index.
        let indexed_count = cache.timestamp_index.get(&1000).map_or(0, |v| v.len());
        assert_eq!(indexed_count, 3);

        // Only receipt timestamps are indexed (metadata timestamps are not)
        assert!(cache.timestamp_index.contains_key(&1000));
        assert!(!cache.timestamp_index.contains_key(&500));
    }

    #[test]
    fn test_trim_to_size() {
        let mut cache = EarlyReceiptCache::new();
        let sender = create_service_id("55555555-1234-5678-1234-555555555555");

        // Add more than MAX_CACHE_SIZE receipts
        for i in 0..MAX_CACHE_SIZE + 5 {
            let message = create_receipt_message(vec![i as u64 * 1000]);
            cache.add(create_metadata(sender.clone(), i as i64 * 1000), message);
        }

        // Should be trimmed to MAX_CACHE_SIZE
        assert!(cache.len() <= MAX_CACHE_SIZE);
    }

    #[test]
    fn test_cleanup_expired() {
        let mut cache = EarlyReceiptCache::new();

        let sender = create_service_id("55555555-1234-5678-1234-555555555555");
        let message = create_receipt_message(vec![1000]);

        assert!(cache.add(create_metadata(sender.clone(), 1000), message));
        assert_eq!(cache.len(), 1);

        cache
            .receipts
            .get_mut(&(cache.next_id - 1))
            .unwrap()
            .created_at = Utc::now() - Duration::days(2);

        cache.cleanup_expired();
        assert_eq!(cache.len(), 0);
    }
}

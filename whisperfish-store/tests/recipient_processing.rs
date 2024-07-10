mod common;

use self::common::*;
use ::phonenumber::PhoneNumber;
use libsignal_service::{ServiceAddress, ServiceIdType};
use rand::Rng;
use rstest::{fixture, rstest};
use std::future::Future;
use uuid::{uuid, Uuid};

const UUID: Uuid = uuid!("dc6bf7f6-9946-4e01-89f6-dc3abdb2f71b");
const UUID2: Uuid = uuid!("c25f3e9a-2cfd-4eb0-8a53-b22eb025667d");

#[fixture]
fn phonenumber() -> ::phonenumber::PhoneNumber {
    let mut rng = rand::thread_rng();
    let e164 = format!("+3247{}", rng.gen_range(4000000..=4999999));
    ::phonenumber::parse(None, e164).unwrap()
}

#[fixture]
fn aci() -> ::libsignal_service::ServiceAddress {
    ServiceAddress::new_aci(Uuid::new_v4())
}

#[fixture]
fn pni() -> ::libsignal_service::ServiceAddress {
    ServiceAddress::new_pni(Uuid::new_v4())
}

#[fixture]
fn storage_with_e164_recipient(
    storage: impl Future<Output = InMemoryDb>,
    phonenumber: PhoneNumber,
) -> impl Future<Output = (InMemoryDb, PhoneNumber)> {
    use futures::prelude::*;
    storage.map(|(storage, _temp_dir)| {
        storage.fetch_or_insert_recipient_by_phonenumber(&phonenumber);

        ((storage, _temp_dir), phonenumber)
    })
}

#[fixture]
fn storage_with_uuid_recipient(
    storage: impl Future<Output = InMemoryDb>,
) -> impl Future<Output = InMemoryDb> {
    use futures::prelude::*;
    storage.map(|(storage, _temp_dir)| {
        storage.fetch_or_insert_recipient_by_address(&ServiceAddress::new_aci(UUID));

        (storage, _temp_dir)
    })
}

#[rstest]
#[tokio::test]
async fn insert_then_fetch_by_e164(
    phonenumber: PhoneNumber,
    storage: impl Future<Output = InMemoryDb>,
) {
    let (storage, _temp_dir) = storage.await;

    let recipient1 = storage.fetch_or_insert_recipient_by_phonenumber(&phonenumber);
    let recipient2 = storage.fetch_or_insert_recipient_by_phonenumber(&phonenumber);
    assert_eq!(recipient1.id, recipient2.id);
    assert_eq!(recipient1.e164, Some(phonenumber));
}

#[rstest]
#[tokio::test]
async fn insert_then_fetch_by_uuid(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let recipient1 = storage.fetch_or_insert_recipient_by_address(&ServiceAddress::new_aci(UUID));
    let recipient2 = storage.fetch_or_insert_recipient_by_address(&ServiceAddress::new_aci(UUID));
    assert_eq!(recipient1.id, recipient2.id);
    assert_eq!(recipient1.uuid, Some(UUID));
}

mod merge_and_fetch {
    use super::*;
    use whisperfish_store::TrustLevel;

    #[rstest]
    #[tokio::test]
    async fn trusted_pair(storage: impl Future<Output = InMemoryDb>, phonenumber: PhoneNumber) {
        let (storage, _temp_dir) = storage.await;

        let recipient = storage.merge_and_fetch_recipient_by_address(
            Some(phonenumber.clone()),
            ServiceAddress::new_aci(UUID),
            TrustLevel::Certain,
        );
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        // Second call should be a no-op
        let recipient_check = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID)),
            None,
            TrustLevel::Certain,
        );
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));
        assert_eq!(recipient_check.id, recipient.id);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_pair(storage: impl Future<Output = InMemoryDb>, phonenumber: PhoneNumber) {
        let (storage, _temp_dir) = storage.await;

        let recipient = storage.merge_and_fetch_recipient_by_address(
            Some(phonenumber.clone()),
            ServiceAddress::new_aci(UUID),
            TrustLevel::Uncertain,
        );

        // When there's no E.164 match, we can save the uncertain-E.164 value too.
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));
    }

    #[rstest]
    #[tokio::test]
    async fn trusted_amend_e164(
        storage_with_e164_recipient: impl Future<Output = (InMemoryDb, PhoneNumber)>,
    ) {
        let ((storage, _temp_dir), phonenumber) = storage_with_e164_recipient.await;

        let recipient = storage.merge_and_fetch_recipient_by_address(
            Some(phonenumber.clone()),
            ServiceAddress::new_aci(UUID),
            TrustLevel::Certain,
        );
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_amend_e164(
        storage_with_e164_recipient: impl Future<Output = (InMemoryDb, PhoneNumber)>,
    ) {
        let ((storage, _temp_dir), phonenumber) = storage_with_e164_recipient.await;

        let recipient_e164 = storage
            .fetch_recipient_by_e164(&phonenumber)
            .expect("e164 in db");
        assert_eq!(recipient_e164.uuid, None);
        assert_eq!(recipient_e164.e164, Some(phonenumber.clone()));

        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID)),
            None,
            TrustLevel::Uncertain,
        );

        assert_eq!(recipient.id, recipient_e164.id);

        assert_eq!(recipient.e164, Some(phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        assert_eq!(storage.fetch_recipients().len(), 1);

        let recipient_uuid = storage
            .fetch_recipient_by_service_address(&ServiceAddress::new_aci(UUID))
            .expect("uuid still in db");
        assert_eq!(recipient.id, recipient_uuid.id);
    }

    #[rstest]
    #[tokio::test]
    async fn trusted_amend_uuid(
        storage_with_uuid_recipient: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage_with_uuid_recipient.await;

        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID)),
            None,
            TrustLevel::Certain,
        );
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_amend_uuid(
        storage_with_uuid_recipient: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage_with_uuid_recipient.await;

        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID)),
            None,
            TrustLevel::Uncertain,
        );

        // Since there were no E.164 match, the phone number was merged
        // despite TrustLevel::Uncertain.
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        // Now check that the e164 does not exist separately.
        assert!(storage.fetch_recipient_by_e164(&phonenumber).is_some());

        assert_eq!(storage.fetch_recipients().len(), 1);
    }
}

mod merge_and_fetch_conflicting_recipients {
    use super::*;
    use whisperfish_store::TrustLevel;

    #[rstest]
    #[tokio::test]
    async fn trusted_disjunct_recipients(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.fetch_or_insert_recipient_by_phonenumber(&phonenumber);
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceAddress::new_aci(UUID));
        // We have two separate recipients.
        assert_ne!(r1.id, r2.id);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // If we now fetch the recipient based on both e164 and uuid, with certainty of their
        // relation,
        // we trigger their merger.
        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID)),
            None,
            TrustLevel::Certain,
        );
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        // Now check that the e164/uuid does not exist separately.
        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_disjunct_recipients(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.fetch_or_insert_recipient_by_phonenumber(&phonenumber);
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceAddress::new_aci(UUID));
        // We have two separate recipients.
        assert_ne!(r1.id, r2.id);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // If we now fetch the recipient based on both e164 and uuid,
        // we trigger their merger even without certainty of their relation,
        // because there is no conflicting data or PNI/ACI set.
        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID)),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(recipient.e164, Some(phonenumber));
        assert_eq!(recipient.id, r2.id);
        assert_eq!(recipient.uuid, Some(UUID));

        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn trusted_recipient_with_new_uuid(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID)),
            None,
            TrustLevel::Certain,
        );
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceAddress {
            uuid: UUID2,
            identity: ServiceIdType::AccountIdentity,
        });
        // We have two separate recipients.
        assert_ne!(r1.id, r2.id);
        assert_eq!(storage.fetch_recipients().len(), 2);
        assert_eq!(r1.e164.as_ref(), Some(&phonenumber));
        assert_eq!(r1.uuid, Some(UUID));

        // If we now fetch the recipient based on both e164 and uuid2, with certainty of their
        // relation,
        // we trigger the move of the phone number.
        // XXX Signal Android then marks the former as "needing refresh". Still need to figure out what
        // that is, but it probably checks with the server than indeed the former UUID doesn't
        // exist anymore, and that the data needs to be moved.
        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(ServiceAddress::new_aci(UUID2)),
            None,
            TrustLevel::Certain,
        );
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID2));

        // Now check that the old recipient still exists.
        assert_eq!(storage.fetch_recipients().len(), 2);

        let recipient = storage
            .fetch_recipient_by_id(r1.id)
            .expect("r1 still exists");
        assert_eq!(recipient.uuid, Some(UUID));
        assert_eq!(recipient.e164.as_ref(), None);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_recipient_with_new_uuid(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage.await;

        let addr1 = ServiceAddress::new_aci(UUID);
        let addr2 = ServiceAddress::new_aci(UUID2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(addr1),
            None,
            TrustLevel::Certain,
        );
        let r2 = storage.fetch_or_insert_recipient_by_address(&addr2);
        // We have two separate recipients.
        assert_ne!(r1.id, r2.id);
        assert_eq!(storage.fetch_recipients().len(), 2);
        assert_eq!(r1.e164.as_ref(), Some(&phonenumber));
        assert_eq!(r1.uuid, Some(UUID));
        assert_eq!(r1.pni, None);
        assert_eq!(r2.e164.as_ref(), None);
        assert_eq!(r2.uuid, Some(UUID2));
        assert_eq!(r2.pni, None);

        // If we now fetch the recipient based on both e164 and uuid2, with uncertainty of their
        // relation,
        // we should get the uuid2 recipient without any other action.
        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(addr2),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(recipient.id, r2.id);
        assert_eq!(recipient.e164.as_ref(), None);
        assert_eq!(recipient.uuid, Some(UUID2));

        // Now check that the old recipient still exists.
        assert_eq!(storage.fetch_recipients().len(), 2);

        let recipient = storage
            .fetch_recipient_by_id(r1.id)
            .expect("r1 still exists");
        assert_eq!(recipient.uuid, Some(UUID));
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
    }

    // allNonMergeTests()

    #[rstest]
    #[tokio::test]
    async fn e164_only_insert(storage: impl Future<Output = InMemoryDb>, phonenumber: PhoneNumber) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            None,
            TrustLevel::Uncertain,
        );
        assert!(r.id > 0);
        assert_eq!(r.uuid, None);
        assert_eq!(r.e164, Some(phonenumber));
        assert_eq!(r.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn pni_only_insert(storage: impl Future<Output = InMemoryDb>, pni: ServiceAddress) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(None, None, Some(pni), TrustLevel::Uncertain);
        assert!(r.id > 0);
        assert_eq!(r.uuid, None);
        assert_eq!(r.e164, None);
        assert_eq!(r.pni, Some(pni.uuid));
    }

    #[rstest]
    #[tokio::test]
    async fn aci_only_insert(storage: impl Future<Output = InMemoryDb>, aci: ServiceAddress) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(None, Some(aci), None, TrustLevel::Uncertain);
        assert!(r.id > 0);
        assert_eq!(r.uuid, Some(aci.uuid));
        assert_eq!(r.e164, None);
        assert_eq!(r.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_insert(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        pni: ServiceAddress,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(pni),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r.id > 0);
        assert_eq!(r.uuid, Some(pni.uuid));
        assert_eq!(r.e164, Some(phonenumber));
        assert_eq!(r.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_aci_insert(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: ServiceAddress,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r.id > 0);
        assert_eq!(r.uuid, Some(aci.uuid));
        assert_eq!(r.e164, Some(phonenumber));
        assert_eq!(r.pni, None);
    }

    #[rstest]
    #[tokio::test]
    // TODO: Figure out PNI verified
    async fn e164_pni_and_aci_insert_pni_unverified(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: ServiceAddress,
        pni: ServiceAddress,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r.id > 0);
        assert_eq!(r.uuid, Some(aci.uuid));
        assert_eq!(r.e164, Some(phonenumber));
        assert_eq!(r.pni, Some(pni.uuid));
    }
}

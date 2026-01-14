mod common;

use self::common::*;
use ::phonenumber::PhoneNumber;
use libsignal_service::protocol::{Aci, Pni, ServiceId};
use rand::Rng;
use rstest::{fixture, rstest};
use std::future::Future;
use uuid::{uuid, Uuid};

const UUID: Uuid = uuid!("dc6bf7f6-9946-4e01-89f6-dc3abdb2f71b");
const UUID2: Uuid = uuid!("c25f3e9a-2cfd-4eb0-8a53-b22eb025667d");

#[fixture]
fn phonenumber() -> ::phonenumber::PhoneNumber {
    let mut rng = rand::rng();
    let e164 = format!("+3247{}", rng.random_range(4000000..=4999999));
    ::phonenumber::parse(None, e164).unwrap()
}

#[fixture]
fn aci() -> Aci {
    Aci::from(Uuid::new_v4())
}

#[fixture]
fn pni() -> Pni {
    Pni::from(Uuid::new_v4())
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
        storage.fetch_or_insert_recipient_by_address(&ServiceId::from(Aci::from(UUID)));

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

    let recipient1 =
        storage.fetch_or_insert_recipient_by_address(&ServiceId::from(Aci::from(UUID)));
    let recipient2 =
        storage.fetch_or_insert_recipient_by_address(&ServiceId::from(Aci::from(UUID)));
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
            ServiceId::from(Aci::from(UUID)),
            TrustLevel::Certain,
        );
        assert_eq!(recipient.e164.as_ref(), Some(&phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        // Second call should be a no-op
        let recipient_check = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(Aci::from(UUID)),
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
            ServiceId::from(Aci::from(UUID)),
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
            ServiceId::from(Aci::from(UUID)),
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
            Some(Aci::from(UUID)),
            None,
            TrustLevel::Uncertain,
        );

        assert_eq!(recipient.id, recipient_e164.id);

        assert_eq!(recipient.e164, Some(phonenumber));
        assert_eq!(recipient.uuid, Some(UUID));

        assert_eq!(storage.fetch_recipients().len(), 1);

        let recipient_uuid = storage
            .fetch_recipient(&ServiceId::from(Aci::from(UUID)))
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
            Some(Aci::from(UUID)),
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
            Some(Aci::from(UUID)),
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
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(Aci::from(UUID)));
        // We have two separate recipients.
        assert_ne!(r1.id, r2.id);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // If we now fetch the recipient based on both e164 and uuid, with certainty of their
        // relation,
        // we trigger their merger.
        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(Aci::from(UUID)),
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
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(Aci::from(UUID)));
        // We have two separate recipients.
        assert_ne!(r1.id, r2.id);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // If we now fetch the recipient based on both e164 and uuid,
        // we trigger their merger even without certainty of their relation,
        // because there is no conflicting data or PNI/ACI set.
        let recipient = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(Aci::from(UUID)),
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
            Some(Aci::from(UUID)),
            None,
            TrustLevel::Certain,
        );
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(Aci::from(UUID2)));
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
            Some(Aci::from(UUID2)),
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

        let addr1 = Aci::from(UUID);
        let addr2 = Aci::from(UUID2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(addr1),
            None,
            TrustLevel::Certain,
        );
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(addr2));
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
    async fn pni_only_insert(storage: impl Future<Output = InMemoryDb>, pni: Pni) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(None, None, Some(pni), TrustLevel::Uncertain);
        assert!(r.id > 0);
        assert_eq!(r.uuid, None);
        assert_eq!(r.e164, None);
        assert_eq!(r.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn aci_only_insert(storage: impl Future<Output = InMemoryDb>, aci: Aci) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(None, Some(aci), None, TrustLevel::Uncertain);
        assert!(r.id > 0);
        assert_eq!(r.uuid, Some(aci.into()));
        assert_eq!(r.e164, None);
        assert_eq!(r.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_insert(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r.id > 0);
        assert_eq!(r.uuid, None);
        assert_eq!(r.e164, Some(phonenumber));
        assert_eq!(r.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_aci_insert(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r.id > 0);
        assert_eq!(r.uuid, Some(aci.into()));
        assert_eq!(r.e164, Some(phonenumber));
        assert_eq!(r.pni, None);
    }

    #[rstest]
    #[tokio::test]
    // TODO: Figure out PNI verified
    async fn e164_pni_and_aci_insert_pni_unverified(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r.id > 0);
        assert_eq!(r.uuid, Some(aci.into()));
        assert_eq!(r.e164, Some(phonenumber));
        assert_eq!(r.pni, Some(pni.into()));
    }

    // allSimpleTests()

    #[rstest]
    #[tokio::test]
    async fn no_match_e164_only(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_pni_only(storage: impl Future<Output = InMemoryDb>, pni: Pni) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni), TrustLevel::Uncertain);
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(None, None, Some(pni), TrustLevel::Uncertain);
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, None);
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_aci_only(storage: impl Future<Output = InMemoryDb>, aci: Aci) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, Some(aci), None, TrustLevel::Uncertain);
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(None, Some(aci), None, TrustLevel::Uncertain);
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, None);
        assert_eq!(r2.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_e164_and_pni(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(phonenumber));
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_e164_and_aci(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber));
        assert_eq!(r2.pni, None);
    }

    #[rstest]
    #[tokio::test]
    #[should_panic]
    async fn no_data_no_match(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        storage.merge_and_fetch_recipient(None, None, None, TrustLevel::Uncertain);
    }

    #[rstest]
    #[tokio::test]
    async fn pni_matches_pni_plus_aci_provided_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 =
            storage.merge_and_fetch_recipient(None, Some(aci), Some(pni), TrustLevel::Uncertain);
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber));
        assert_eq!(r2.pni, Some(pni.into()));

        // TODO: pni_matches_pni_plus_aci_provided_pni_session?
        // TODO: pni_matches_pni_plus_aci_provided_pni_session_pni_verified?
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_all_fields(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r1.id, r2.id);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        // TODO: full_match?
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_all_fields_provided(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_aci_provided(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber));
        assert_eq!(r2.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_pni_provided(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber));
        assert_eq!(r2.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_all_provided_different_aci(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let aci_2 = Aci::from(Uuid::new_v4());
        assert_ne!(aci, aci_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci_2),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci_2.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r2.id > 0);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));

        let r1 = storage.fetch_recipient(&aci_2.into()).unwrap();
        let r2 = storage.fetch_recipient(&aci.into()).unwrap();

        assert!(r1.id > 0);
        assert!(r2.id > 0);
        assert_ne!(r1.id, r2.id);

        assert_eq!(r1.uuid, Some(aci_2.into()));
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, None);

        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber));
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_aci_provided_different_aci(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let aci_2 = Aci::from(Uuid::new_v4());
        assert_ne!(aci, aci_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci_2),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r2.id > 0);
        assert_eq!(r2.uuid, Some(aci_2.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, None);

        let r1 = storage.fetch_recipient(&aci.into()).unwrap();
        assert!(r1.id > 0);
        assert_ne!(r1.id, r2.id);

        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_matches_all_provided_new_aci_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
        // TODO: e164_and_pni_matches_all_provided_new_aci_existing_pni_session
        // TODO: e164_and_pni_matches_all_provided_new_aci_existing_pni_session_pni_verified
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_matches_all_provided_new_pni(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn pni_matches_all_provided_new_e164_and_aci_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni), TrustLevel::Uncertain);
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
        // TODO: pni_matches_all_provided_new_e164_and_aci_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn pni_and_aci_matches_all_provided_new_e164(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 =
            storage.merge_and_fetch_recipient(None, Some(aci), Some(pni), TrustLevel::Uncertain);
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_aci_matches_e164_and_aci_provided_nothing_new(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_all_provided_new_e164_and_pni(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, Some(aci), None, TrustLevel::Uncertain);
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_e164_and_aci_provided(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, None);
    }

    // TODO: aci_matches_local_user_chane_self_false

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_pni_provided_pni_changes_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
    ) {
        let (storage, _temp_dir) = storage.await;

        let pni_1 = pni();
        let pni_2 = pni();
        assert_ne!(pni_1, pni_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni_2),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, Some(pni_2.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni_1),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni_1.into()));

        // TODO: e164_matches_e164_and_pni_provided_pni_changes_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_matches_all_provided_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));

        // TODO: e164_and_pni_matches_all_provided_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_provided_pni_changed(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
    }

    #[rstest]
    #[tokio::test]
    async fn pni_matches_all_provided_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
        phonenumber: PhoneNumber,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni), TrustLevel::Uncertain);
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, None);
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
        // TODO: pni_matches_all_provided_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn pni_matches_no_existing_pni_session_changes_number(
        storage: impl Future<Output = InMemoryDb>,
        pni: Pni,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let phonenumber_1 = phonenumber();
        let phonenumber_2 = phonenumber();
        assert_ne!(phonenumber_1, phonenumber_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber_2.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(phonenumber_2.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber_1.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber_1.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
        // TODO: phone number change event
        // TODO: pni_matches_existing_pni_session_changes_number
    }

    #[rstest]
    #[tokio::test]
    async fn pni_and_aci_matches_change_number(
        storage: impl Future<Output = InMemoryDb>,
        pni: Pni,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let phonenumber_1 = phonenumber();
        let phonenumber_2 = phonenumber();
        assert_ne!(phonenumber_1, phonenumber_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber_2.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber_2.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber_1.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber_1.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
        // TODO: phone number change event
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_all_procided_change_number(
        storage: impl Future<Output = InMemoryDb>,
        pni: Pni,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let phonenumber_1 = phonenumber();
        let phonenumber_2 = phonenumber();
        assert_ne!(phonenumber_1, phonenumber_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber_2.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber_2.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber_1.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber_1.clone()));
        assert_eq!(r2.pni, Some(pni.into()));
        // TODO: phone number change event
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_e164_and_aci_provided_change_number(
        storage: impl Future<Output = InMemoryDb>,
        aci: Aci,
    ) {
        let (storage, _temp_dir) = storage.await;

        let phonenumber_1 = phonenumber();
        let phonenumber_2 = phonenumber();
        assert_ne!(phonenumber_1, phonenumber_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(phonenumber_2.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(phonenumber_2.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(phonenumber_1.clone()),
            Some(aci),
            None,
            TrustLevel::Uncertain,
        );
        assert_eq!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, Some(phonenumber_1.clone()));
        assert_eq!(r2.pni, None);
        // TODO: phone number change event
    }

    #[rstest]
    #[tokio::test]
    async fn steal_pni_is_changed(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let aci = aci();
        let e164_a = phonenumber();
        let e164_b = phonenumber();
        assert_ne!(e164_a, e164_b);
        let pni_a = pni();
        let pni_b = pni();
        assert_ne!(pni_a, pni_b);

        let r1 = storage.merge_and_fetch_recipient(
            Some(e164_a.clone()),
            Some(aci),
            Some(pni_b),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci.into()));
        assert_eq!(r1.e164, Some(e164_a.clone()));
        assert_eq!(r1.pni, Some(pni_b.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(e164_b.clone()),
            None,
            Some(pni_a),
            TrustLevel::Uncertain,
        );
        assert!(r2.id > 0);
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(e164_b.clone()));
        assert_eq!(r2.pni, Some(pni_a.into()));

        let r3 = storage.merge_and_fetch_recipient(
            Some(e164_a.clone()),
            None,
            Some(pni_a),
            TrustLevel::Uncertain,
        );
        assert_eq!(r3.id, r1.id);
        assert_eq!(r3.uuid, Some(aci.into()));
        assert_eq!(r3.e164, Some(e164_a.clone()));
        assert_eq!(r3.pni, Some(pni_a.into()));

        let r4 = storage.fetch_recipient_by_e164(&e164_b).unwrap();
        assert_eq!(r4.id, r2.id);
        assert_eq!(r4.uuid, None);
        assert_eq!(r4.e164, Some(e164_b.clone()));
        assert_eq!(r4.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_pni_is_changed_aci_left_behind(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let aci_1 = aci();
        let e164_1 = phonenumber();
        let e164_2 = phonenumber();
        assert_ne!(e164_1, e164_2);
        let pni_1 = pni();
        let pni_2 = pni();
        assert_ne!(pni_1, pni_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(e164_2.clone()),
            Some(aci_1),
            Some(pni_1),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci_1.into()));
        assert_eq!(r1.e164, Some(e164_2.clone()));
        assert_eq!(r1.pni, Some(pni_1.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            None,
            Some(pni_2),
            TrustLevel::Uncertain,
        );
        assert!(r2.id > 0);
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(e164_1.clone()));
        assert_eq!(r2.pni, Some(pni_2.into()));

        let r3 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            None,
            Some(pni_1),
            TrustLevel::Uncertain,
        );
        assert_eq!(r3.id, r2.id);
        assert_eq!(r3.uuid, None);
        assert_eq!(r3.e164, Some(e164_1.clone()));
        assert_eq!(r3.pni, Some(pni_1.into()));

        let r4 = storage.fetch_recipient_by_e164(&e164_2).unwrap();
        assert_eq!(r4.id, r1.id);
        assert_eq!(r4.uuid, Some(aci_1.into()));
        assert_eq!(r4.e164, Some(e164_2.clone()));
        assert_eq!(r4.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_and_pni_matches_e164_and_pni_provided_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
    ) {
        let (storage, _temp_dir) = storage.await;

        let e164_1 = phonenumber();
        let e164_2 = phonenumber();
        assert_ne!(e164_1, e164_2);
        let pni_1 = pni();
        let pni_2 = pni();
        assert_ne!(pni_1, pni_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            None,
            Some(pni_2),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(e164_1.clone()));
        assert_eq!(r1.pni, Some(pni_2.into()));

        let r2 = storage.merge_and_fetch_recipient(
            Some(e164_2.clone()),
            None,
            Some(pni_1),
            TrustLevel::Uncertain,
        );
        assert!(r2.id > 0);
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(e164_2.clone()));
        assert_eq!(r2.pni, Some(pni_1.into()));

        let r3 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            None,
            Some(pni_1),
            TrustLevel::Uncertain,
        );
        assert_eq!(r3.id, r1.id);
        assert_eq!(r3.uuid, None);
        assert_eq!(r3.e164, Some(e164_1.clone()));
        assert_eq!(r3.pni, Some(pni_1.into()));

        let r4 = storage.fetch_recipient_by_e164(&e164_2).unwrap();
        assert_eq!(r4.id, r2.id);
        assert_eq!(r4.uuid, None);
        assert_eq!(r4.e164, Some(e164_2.clone()));
        assert_eq!(r4.pni, None);
        // TODO: steal_e164_and_pni_matches_e164_and_pni_provided_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_plus_pni_and_aci_but_e164_record_has_separate_e164(
        storage: impl Future<Output = InMemoryDb>,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let e164_1 = phonenumber();
        let e164_2 = phonenumber();
        assert_ne!(e164_1, e164_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(e164_2.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(e164_2.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(None, Some(aci), None, TrustLevel::Uncertain);
        assert!(r2.id > 0);
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, None);
        assert_eq!(r2.pni, None);

        let r3 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r3.id, r2.id);
        assert_eq!(r3.uuid, Some(aci.into()));
        assert_eq!(r3.e164, Some(e164_1.clone()));
        assert_eq!(r3.pni, Some(pni.into()));

        let r4 = storage.fetch_recipient_by_e164(&e164_2).unwrap();
        assert_eq!(r4.id, r1.id);
        assert_eq!(r4.uuid, None);
        assert_eq!(r4.e164, Some(e164_2.clone()));
        assert_eq!(r4.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_plus_pni_and_aci_and_e164_record_has_separate_e164(
        storage: impl Future<Output = InMemoryDb>,
        aci: Aci,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let e164_1 = phonenumber();
        let e164_2 = phonenumber();
        assert_ne!(e164_1, e164_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(e164_2.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(e164_2.clone()));
        assert_eq!(r1.pni, Some(pni.into()));

        let r2 = storage.merge_and_fetch_recipient(None, Some(aci), None, TrustLevel::Uncertain);
        assert!(r2.id > 0);
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci.into()));
        assert_eq!(r2.e164, None);
        assert_eq!(r2.pni, None);

        let r3 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            Some(aci),
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_eq!(r3.id, r2.id);
        assert_eq!(r3.uuid, Some(aci.into()));
        assert_eq!(r3.e164, Some(e164_1.clone()));
        assert_eq!(r3.pni, Some(pni.into()));

        let r4 = storage.fetch_recipient_by_e164(&e164_2).unwrap();
        assert_eq!(r4.id, r1.id);
        assert_eq!(r4.uuid, None);
        assert_eq!(r4.e164, Some(e164_2));
        assert_eq!(r4.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_plus_pni_match_e164_and_aci_provided_change_number(
        storage: impl Future<Output = InMemoryDb>,
    ) {
        let (storage, _temp_dir) = storage.await;

        let e164_1 = phonenumber();
        let e164_2 = phonenumber();
        assert_ne!(e164_1, e164_2);

        let aci_1 = aci();
        let aci_2 = aci();
        assert_ne!(aci_1, aci_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(e164_2.clone()),
            Some(aci_1),
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, Some(aci_1.into()));
        assert_eq!(r1.e164, Some(e164_2.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            Some(aci_2),
            None,
            TrustLevel::Uncertain,
        );
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.uuid, Some(aci_2.into()));
        assert_eq!(r2.e164, Some(e164_1.clone()));
        assert_eq!(r2.pni, None);

        let r3 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            Some(aci_1),
            None,
            TrustLevel::Certain,
        );
        assert_eq!(r3.id, r1.id);
        assert_eq!(r3.uuid, Some(aci_1.into()));
        assert_eq!(r3.e164, Some(e164_1.clone()));
        assert_eq!(r3.pni, None);

        let r4 = storage.fetch_recipient(&aci_2.into()).unwrap();
        assert_eq!(r4.id, r2.id);
        assert_eq!(r4.uuid, Some(aci_2.into()));
        assert_eq!(r4.e164, None);
        assert_eq!(r4.pni, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_and_pni_plus_aci_no_aci_provided_no_pni_session(
        storage: impl Future<Output = InMemoryDb>,
        pni: Pni,
    ) {
        let (storage, _temp_dir) = storage.await;

        let e164_1 = phonenumber();
        let e164_2 = phonenumber();
        assert_ne!(e164_1, e164_2);

        let r1 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            None,
            None,
            TrustLevel::Uncertain,
        );
        assert!(r1.id > 0);
        assert_eq!(r1.uuid, None);
        assert_eq!(r1.e164, Some(e164_1.clone()));
        assert_eq!(r1.pni, None);

        let r2 = storage.merge_and_fetch_recipient(
            Some(e164_2.clone()),
            None,
            Some(pni),
            TrustLevel::Uncertain,
        );
        assert_ne!(r2.id, r1.id);
        assert_eq!(r2.uuid, None);
        assert_eq!(r2.e164, Some(e164_2.clone()));
        assert_eq!(r2.pni, Some(pni.into()));

        let r3 = storage.merge_and_fetch_recipient(
            Some(e164_1.clone()),
            None,
            Some(pni),
            TrustLevel::Certain,
        );
        assert_eq!(r3.id, r1.id);
        assert_eq!(r3.uuid, None);
        assert_eq!(r3.e164, Some(e164_1.clone()));
        assert_eq!(r3.pni, Some(pni.into()));

        let r4 = storage.fetch_recipient_by_e164(&e164_2).unwrap();
        assert_eq!(r4.id, r2.id);
        assert_eq!(r4.uuid, None);
        assert_eq!(r4.e164, Some(e164_2));
        assert_eq!(r4.pni, None);
        // TODO: steal_e164_and_pni_plus_aci_no_aci_provided_pni_session_exists
    }
}

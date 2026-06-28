mod common;

use self::common::*;
use ::phonenumber::PhoneNumber;
use libsignal_service::protocol::{Aci, Pni, ServiceId};
use rstest::{fixture, rstest};
use std::{future::Future, str::FromStr};
use uuid::uuid;

// Define all ACIs, PNIs and E164s beforehand
// so we don't have to assert_ne any of them.
#[rustfmt::skip]
fn pni(id: usize) -> Pni {
    Pni::from(match id {
        1 => uuid!("caebd99c-e95a-40ef-b9b2-67776ad810dc"),
        2 => uuid!("460d43fe-fd38-492b-9e0f-a8913d69858f"),
        3 => uuid!("4b3570a9-0959-4abe-b498-90dfedf57ff1"),
        4 => uuid!("92f62105-d1dd-4817-910c-539bbb9ba7f5"),
        _ => unreachable!(),
    })
}

#[rustfmt::skip]
fn aci(id: usize) -> Aci {
    Aci::from(match id {
        1 => uuid!("3f84f7bf-6238-471a-9828-fa60c66fc006"),
        2 => uuid!("05556732-7c64-42d4-8afb-4e7c140e4872"),
        3 => uuid!("ec28b2d0-02c9-4781-b1bd-2e0f790af7ec"),
        4 => uuid!("233214ad-e994-4b2b-a40c-4d00f9fa23ba"),
        _ => unreachable!(),
    })
}

#[rustfmt::skip]
fn e164(id: usize) -> PhoneNumber {
    PhoneNumber::from_str(match id {
        1 => "+32474091150",
        2 => "+34666777888",
        3 => "+34612345678",
        4 => "+330631966543",
        _ => unreachable!(),
    }).unwrap()
}

// Inspired by verify() from Signal Android tests
#[rustfmt::skip]
enum Id {
    Nz,
    Eq(i32),
    Ne(i32),
}

#[rustfmt::skip]
fn verify(
    r: &whisperfish_store::orm::Recipient,
    id: Id,
    tel: Option<&PhoneNumber>,
    pni: Option<&Pni>,
    aci: Option<&Aci>,
) {
    match id {
        Id::Nz => assert_ne!(r.id, 0, "id should be 0"),
        Id::Eq(id) => assert_eq!(r.id, id, "id's should be equal"),
        Id::Ne(id) => assert_ne!(r.id, id, "id's should not be equal"),
    }
    assert_eq!(r.uuid.map(Aci::from ).as_ref(), aci, "aci's should be equal");
    assert_eq!(r.pni.map(Pni::from  ).as_ref(), pni, "pni's should be equal");
    assert_eq!(r.e164.as_ref(), tel, "e164's should be equal");
}

#[fixture]
fn storage_with_e164_recipient(
    storage: impl Future<Output = InMemoryDb>,
) -> impl Future<Output = (InMemoryDb, PhoneNumber)> {
    use futures::prelude::*;
    storage.map(|(storage, _temp_dir)| {
        storage.fetch_or_insert_recipient_by_phonenumber(&e164(1));

        ((storage, _temp_dir), e164(1))
    })
}

#[fixture]
fn storage_with_aci_recipient(
    storage: impl Future<Output = InMemoryDb>,
) -> impl Future<Output = InMemoryDb> {
    use futures::prelude::*;
    storage.map(|(storage, _temp_dir)| {
        storage.fetch_or_insert_recipient_by_address(&ServiceId::from(aci(1)));
        (storage, _temp_dir)
    })
}

#[rstest]
#[tokio::test]
async fn insert_then_fetch_by_e164(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let recipient1 = storage.fetch_or_insert_recipient_by_phonenumber(&e164(1));
    let recipient2 = storage.fetch_or_insert_recipient_by_phonenumber(&e164(1));
    assert_eq!(recipient1.id, recipient2.id);
    assert_eq!(recipient1.e164, Some(e164(1)));
}

#[rstest]
#[tokio::test]
async fn insert_then_fetch_by_aci(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let r1 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(aci(1)));
    let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(aci(1)));
    verify(&r2, Id::Eq(r1.id), None, None, Some(&aci(1)));
}

#[rustfmt::skip]
mod merge_and_fetch {
    use super::*;
    use whisperfish_store::TrustLevel;

    #[rstest]
    #[tokio::test]
    async fn trusted_pair(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient_by_address(Some(e164(1)), ServiceId::from(aci(1)), TrustLevel::Certain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));

        // Second call should be a no-op
        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Certain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_pair(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient_by_address(Some(e164(1)), ServiceId::from(aci(1)), TrustLevel::Uncertain);

        // When there's no E.164 match, we can save the uncertain-E.164 value too.
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn trusted_amend_e164(
        storage_with_e164_recipient: impl Future<Output = (InMemoryDb, PhoneNumber)>) {
        let ((storage, _temp_dir), _e164) = storage_with_e164_recipient.await;

        let r1 = storage.merge_and_fetch_recipient_by_address(Some(e164(1)), ServiceId::from(aci(1)), TrustLevel::Certain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));
        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_amend_e164(
        storage_with_e164_recipient: impl Future<Output = (InMemoryDb, PhoneNumber)>) {
        let ((storage, _temp_dir), phonenumber) = storage_with_e164_recipient.await;

        let r1 = storage.fetch_recipient_by_e164(&phonenumber).unwrap();
        verify(&r1, Id::Nz, Some(&e164(1)), None, None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(3)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(3)));

        let r3 = storage.fetch_recipient(&ServiceId::from(aci(3))).unwrap();
        assert_eq!(r2.id, r3.id);

        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn trusted_amend_aci(storage_with_aci_recipient: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage_with_aci_recipient.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Certain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));
        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_amend_aci(storage_with_aci_recipient: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage_with_aci_recipient.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);

        // Since there were no E.164 match, the phone number was merged
        // despite TrustLevel::Uncertain.
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));
        assert_eq!(storage.fetch_recipients().len(), 1);
    }
}

#[rustfmt::skip]
mod merge_and_fetch_conflicting_recipients {
    use super::*;
    use whisperfish_store::TrustLevel;

    #[rstest]
    #[tokio::test]
    async fn trusted_disjunct_recipients(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let _r = storage.fetch_or_insert_recipient_by_phonenumber(&e164(3));
        let r1 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(aci(2)));
        assert_eq!(storage.fetch_recipients().len(), 2);

        // If we now fetch the recipient based on both e164 and uuid, with certainty of their
        // relation, we trigger their merger.
        let r2 = storage.merge_and_fetch_recipient(Some(e164(3)), Some(aci(2)), None, TrustLevel::Certain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(3)), None, Some(&aci(2)));
        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_disjunct_recipients(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let _r = storage.fetch_or_insert_recipient_by_phonenumber(&e164(4));
        let r1 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(aci(1)));
        assert_eq!(storage.fetch_recipients().len(), 2);

        // If we now fetch the recipient based on both e164 and uuid,
        // we trigger their merger even without certainty of their relation,
        // because there is no conflicting data or PNI/ACI set.
        let r2 = storage.merge_and_fetch_recipient(Some(e164(4)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(4)), None, Some(&aci(1)));
        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn trusted_recipient_with_new_aci(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(4)), None, TrustLevel::Certain);
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(aci(2)));
        // We have two separate recipients.
        assert_eq!(storage.fetch_recipients().len(), 2);
        verify(&r2, Id::Ne(r1.id), None, None, Some(&aci(2)));

        // If we now fetch the recipient based on both e164 and uuid2, with certainty of their relation,
        // we trigger the move of the phone number.
        // XXX Signal Android then marks the former as "needing refresh". Still need to figure out what
        // that is, but it probably checks with the server than indeed the former UUID doesn't
        // exist anymore, and that the data needs to be moved.
        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), None, TrustLevel::Certain);
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), None, Some(&aci(2)));

        // Now check that the old recipient still exists.
        assert_eq!(storage.fetch_recipients().len(), 2);

        let r4 = storage.fetch_recipient_by_id(r1.id).unwrap();
        verify(&r4, Id::Eq(r1.id), None, None, Some(&aci(4)));
    }

    #[rstest]
    #[tokio::test]
    async fn untrusted_recipient_with_new_aci(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Certain);
        let r2 = storage.fetch_or_insert_recipient_by_address(&ServiceId::from(aci(2)));

        assert_eq!(storage.fetch_recipients().len(), 2);

        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));
        verify(&r2, Id::Ne(r1.id), None, None, Some(&aci(2)));

        // If we now fetch the recipient based on both e164 and uuid2, with uncertainty of their
        // relation,
        // we should get the uuid2 recipient without any other action.
        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), None, TrustLevel::Uncertain);
        verify(&r3, Id::Eq(r2.id), None, None, Some(&aci(2)));

        // Now check that the old recipient still exists.
        assert_eq!(storage.fetch_recipients().len(), 2);

        let r4 = storage.fetch_recipient_by_id(r1.id).unwrap();
        verify(&r4, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));
    }

    // allNonMergeTests()

    #[rstest]
    #[tokio::test]
    async fn e164_only_insert(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        verify(&r, Id::Nz, Some(&e164(1)), None, None);
    }

    #[rstest]
    #[tokio::test]
    async fn pni_only_insert(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r, Id::Nz, None, Some(&pni(1)), None);
    }

    #[rstest]
    #[tokio::test]
    async fn aci_only_insert(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(None, Some(aci(4)), None, TrustLevel::Uncertain);
        verify(&r, Id::Nz, None, None, Some(&aci(4)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_insert(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r, Id::Nz, Some(&e164(1)), Some(&pni(1)), None);
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_aci_insert(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(4)), None, TrustLevel::Uncertain);
        verify(&r, Id::Nz, Some(&e164(1)), None, Some(&aci(4)));
    }

    #[rstest]
    #[tokio::test]
    // TODO: Figure out PNI verified
    async fn e164_pni_and_aci_insert_pni_unverified(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), Some(pni(2)), TrustLevel::Uncertain);
        verify(&r, Id::Nz, Some(&e164(1)), Some(&pni(2)), Some(&aci(2)));
    }

    // allSimpleTests()

    #[rstest]
    #[tokio::test]
    async fn no_match_e164_only(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, None);
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_pni_only(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni(3)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, None, Some(&pni(3)), None);

        let r2 = storage.merge_and_fetch_recipient(None, None, Some(pni(3)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), None, Some(&pni(3)), None);
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_aci_only(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, Some(aci(3)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, None, None, Some(&aci(3)));

        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(3)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), None, None, Some(&aci(3)));
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_e164_and_pni(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(4)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), Some(&pni(4)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(4)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(2)), Some(&pni(4)), None);
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_e164_and_aci(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None,TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(2)), None, Some(&aci(1)));
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
    async fn pni_matches_pni_plus_aci_provided_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        // TODO: pni_matches_pni_plus_aci_provided_pni_session?
        // TODO: pni_matches_pni_plus_aci_provided_pni_session_pni_verified?
    }

    #[rstest]
    #[tokio::test]
    async fn no_match_all_fields(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_all_fields_provided(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_aci_provided(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_pni_provided(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_all_provided_different_aci(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(2)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        let r1 = storage.fetch_recipient(&aci(2).into()).unwrap();
        verify(&r1, Id::Nz, None, None, Some(&aci(2)));

        let r2 = storage.fetch_recipient(&aci(1).into()).unwrap();
        verify(&r2, Id::Ne(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_aci_provided_different_aci(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), Some(&e164(1)), None, Some(&aci(2)));

        let r1 = storage.fetch_recipient(&aci(1).into()).unwrap();
        verify(&r1, Id::Ne(r2.id), None, None, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_matches_all_provided_new_aci_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        // TODO: e164_and_pni_matches_all_provided_new_aci_existing_pni_session
        // TODO: e164_and_pni_matches_all_provided_new_aci_existing_pni_session_pni_verified
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_matches_all_provided_new_pni(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));


        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn pni_matches_all_provided_new_e164_and_aci_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, None, Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        // TODO: pni_matches_all_provided_new_e164_and_aci_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn pni_and_aci_matches_all_provided_new_e164(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 =
            storage.merge_and_fetch_recipient(None, Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, None, Some(&pni(1)), Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_aci_matches_e164_and_aci_provided_nothing_new(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_all_provided_new_e164_and_pni(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, None, None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_e164_and_aci_provided(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));
    }

    // TODO: aci_matches_local_user_chane_self_false
    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_and_pni_provided_pni_changes_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(4)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), Some(&pni(4)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(3)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(3)), None);
        // TODO: e164_matches_e164_and_pni_provided_pni_changes_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn e164_and_pni_matches_all_provided_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        // TODO: e164_and_pni_matches_all_provided_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn e164_matches_e164_provided_pni_changed(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), None);
    }

    #[rstest]
    #[tokio::test]
    async fn pni_matches_all_provided_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, None, Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        // TODO: pni_matches_all_provided_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn pni_matches_no_existing_pni_session_changes_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        // TODO: phone number change event
        // TODO: pni_matches_existing_pni_session_changes_number
    }

    #[rstest]
    #[tokio::test]
    async fn pni_and_aci_matches_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), Some(&pni(1)), Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        // TODO: phone number change event
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_all_procided_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        // TODO: phone number change event
    }

    #[rstest]
    #[tokio::test]
    async fn aci_matches_e164_and_aci_provided_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));
        // TODO: phone number change event
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "steal, pni is changed"
    async fn steal_pni_is_changed(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(2)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), Some(&pni(2)), Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), Some(&e164(2)), Some(&pni(1)), None);

        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r3, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        let r4 = storage.fetch_recipient_by_e164(&e164(2)).unwrap();
        verify(&r4, Id::Eq(r2.id), Some(&e164(2)), None, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_pni_is_changed_aci_left_behind(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), Some(&pni(1)), Some(&(aci(1))));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(2)), TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), Some(&e164(1)), Some(&pni(2)), None);

        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), None);

        let r4 = storage.fetch_recipient_by_e164(&e164(2)).unwrap();
        verify(&r4, Id::Eq(r1.id), Some(&e164(2)), None, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_and_pni_matches_e164_and_pni_provided_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(2)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), Some(&pni(2)), None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), Some(&e164(2)), Some(&pni(1)), None);

        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r3, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), None);

        let r4 = storage.fetch_recipient_by_e164(&e164(2)).unwrap();
        verify(&r4, Id::Eq(r2.id), Some(&e164(2)), None, None);
        // TODO: steal_e164_and_pni_matches_e164_and_pni_provided_existing_pni_session
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_plus_pni_and_aci_but_e164_record_has_separate_e164(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), None, None, Some(&aci(1)));

        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        let r4 = storage.fetch_recipient_by_e164(&e164(2)).unwrap();
        verify(&r4, Id::Eq(r1.id), Some(&e164(2)), None, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_plus_pni_and_aci_and_e164_record_has_separate_e164(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), Some(&pni(1)), None);

        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), None, None, Some(&aci(1)));

        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        let r4 = storage.fetch_recipient_by_e164(&e164(2)).unwrap();
        verify(&r4, Id::Eq(r1.id), Some(&e164(2)), None, None);
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_plus_pni_match_e164_and_aci_provided_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(2)), None, Some(&aci(1)));

        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), Some(&e164(1)), None, Some(&aci(2)));

        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Certain);
        verify(&r3, Id::Eq(r1.id), Some(&e164(1)), None, Some(&aci(1)));

        let r4 = storage.fetch_recipient(&aci(2).into()).unwrap();
        verify(&r4, Id::Eq(r2.id), None, None, Some(&aci(2)));
    }

    #[rstest]
    #[tokio::test]
    async fn steal_e164_and_pni_plus_aci_no_aci_provided_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        verify(&r1, Id::Nz, Some(&e164(1)), None, None);

        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), None, Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), Some(&e164(2)), Some(&pni(1)), None);

        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Certain);
        verify(&r3, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), None);

        let r4 = storage.fetch_recipient_by_e164(&e164(2)).unwrap();
        verify(&r4, Id::Eq(r2.id), Some(&e164(2)), None, None);
        // TODO: steal_e164_and_pni_plus_aci_no_aci_provided_pni_session_exists
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "steal, e164+pni+aci & e164+aci, no pni provided, change number"
    async fn steal_e164_pni_aci_and_e164_aci_no_pni_provided_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, ACI_A)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);

        // given(E164_B, null, ACI_B)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(2)), None, TrustLevel::Uncertain);

        // process(E164_A, null, ACI_B)
        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), None, TrustLevel::Certain);

        // expect(null, PNI_A, ACI_A) — r1 loses its e164
        let r4 = storage.fetch_recipient(&aci(1).into()).unwrap();
        verify(&r4, Id::Eq(r1.id), None /*E164*/, Some(&pni(1)), Some(&aci(1)));

        // expect(E164_A, null, ACI_B) — r2 gains e164_1
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), None /*PNI*/, Some(&aci(2)));

        // TODO: expectChangeNumberEvent()
    }
    #[rstest]
    #[tokio::test]
    /// Signal Android: "steal, e164+pni & aci, no pni provided, no pni session"
    async fn steal_e164_pni_and_aci_no_pni_provided_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);

        // given(null, null, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);

        // process(E164_A, null, ACI_A)
        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Certain);

        // expect(null, PNI_A, null) — r1 loses e164, keeps pni
        let r4 = storage.fetch_recipient_by_id(r1.id).unwrap();
        verify(&r4, Id::Eq(r1.id), None /*E164*/, Some(&pni(1)), None);

        // expect(E164_A, null, ACI_A) — r2 gains e164
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), None /*PNI*/, Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "steal, e164+pni+aci & pni+aci, all provided, aci sessions but not pni sessions, no SSE expected"
    async fn steal_e164_pni_aci_and_pni_aci_all_provided_aci_sessions_no_sse(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, ACI_A)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);

        // given(null, PNI_B, ACI_B)
        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(2)), Some(pni(2)), TrustLevel::Uncertain);
        verify(&r2, Id::Ne(r1.id), None, Some(&pni(2)), Some(&aci(2)));
        // process(E164_A, PNI_B, ACI_A)
        // expect(E164_A, PNI_B, ACI_A) — r1 gets new pni
        // expect(null, null, ACI_B)    — r2 loses pni
        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(2)), TrustLevel::Certain);
        verify(&r3, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(2)), Some(&aci(1)));

        let r2_updated = storage.fetch_recipient(&aci(2).into()).unwrap();
        verify(&r2_updated, Id::Eq(r2.id), None /*E164*/, None /*PNI*/, Some(&aci(2)));
        // TODO: expectNoSessionSwitchoverEvent()
    }

    // -------------------------------------------------------------------------
    // Merge tests
    // -------------------------------------------------------------------------

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164 & aci"
    async fn merge_e164_and_aci(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, null, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);

        // given(null, null, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);

        // process(E164_A, null, ACI_A) → merge into ACI record; e164 record deleted
        let r3 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Certain);
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), None /*PNI*/, Some(&aci(1)));

        // r1 should have been deleted (merged into r2)
        assert!(storage.fetch_recipient_by_id(r1.id).is_none());
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent(E164_A)
    }

    /*
    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164 & pni & aci, all provided"
    async fn merge_e164_and_pni_and_aci_all_provided(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, null, null)
        storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        // given(null, PNI_A, null)
        storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);
        // given(null, null, ACI_A)
        storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 3);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, ACI_A) → merge all three into ACI record
        let r = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Certain);
        verify(&r, Id::Nz, Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent(E164_A)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164 & pni, no aci provided"
    async fn merge_e164_and_pni_no_aci_provided(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, null, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        // given(null, PNI_A, null)
        let _r = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, null) → merge pni record into e164 record
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Certain);
        verify(&r_out, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), None);

        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent("")
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164 & pni, aci provided, no pni session"
    async fn merge_e164_and_pni_aci_provided_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, null, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        // given(null, PNI_A, null)
        let r2 = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, ACI_A) → merge; e164 record wins, pni record deleted
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Certain);
        verify(&r_out, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        assert!(storage.fetch_recipient_by_id(r2.id).is_none());
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent("")
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164+pni & pni, no aci provided"
    async fn merge_e164_pni_and_pni_no_aci_provided(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_B, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(2)), TrustLevel::Uncertain);
        // given(null, PNI_A, null)
        let r2 = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, null) → r2 (pni_1 record) merges into r1 (e164 record),
        // pni_2 is replaced by pni_1
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Certain);
        verify(&r_out, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), None);

        assert!(storage.fetch_recipient_by_id(r2.id).is_none());
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent("")
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164+pni & aci, no pni session"
    async fn merge_e164_pni_and_aci_no_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        // given(null, null, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, ACI_A) → e164 record deleted, aci record wins
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Certain);
        verify(&r_out, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        assert!(storage.fetch_recipient_by_id(r1.id).is_none());
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent(E164_A)
    }
    */

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164 & e164+aci, change number"
    async fn merge_e164_and_e164_aci_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, null, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, None, TrustLevel::Uncertain);
        // given(E164_B, null, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None, TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // process(E164_A, null, ACI_A) → r1 deleted, r2 gains e164_1, loses e164_2
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Certain);
        verify(&r_out, Id::Eq(r2.id), Some(&e164(1)), None /*PNI*/, Some(&aci(1)));

        assert!(storage.fetch_recipient_by_id(r1.id).is_none());
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectChangeNumberEvent()
        // TODO: expectThreadMergeEvent(E164_A)
    }

    /*
    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164 follows pni+aci"
    async fn merge_e164_follows_pni_and_aci(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null)
        let _r = storage.merge_and_fetch_recipient(Some(e164(1)), None,
            Some(pni(1)), TrustLevel::Uncertain);
        // given(null, null, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), None, TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(null, PNI_A, ACI_A) — no e164 explicitly provided, but e164 "follows" pni
        // expect(E164_A, PNI_A, ACI_A)
        let r_out = storage.merge_and_fetch_recipient(None, Some(aci(1)), Some(pni(1)), TrustLevel::Certain);
        verify(&r_out, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent(E164_A)
        // TODO: expectPniVerified() (pniVerified = true in Signal Android)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164 + pni reassigned, aci abandoned"
    async fn merge_e164_pni_reassigned_aci_abandoned(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, ACI_A)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        // given(E164_B, PNI_B, ACI_B)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(2)), Some(pni(2)), TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, ACI_B)
        // ACI_B moves to E164_A+PNI_A; ACI_A is abandoned (loses e164+pni)
        // expect(null, null, ACI_A)
        // expect(E164_A, PNI_A, ACI_B)
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(2)), Some(pni(1)), TrustLevel::Certain);
        verify(&r_out, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(2)));

        let r1_updated = storage.fetch_recipient(&aci(1).into()).unwrap();
        verify(&r1_updated, Id::Eq(r1.id), None /*E164*/, None /*PNI*/, Some(&aci(1)));

        assert_eq!(storage.fetch_recipients().len(), 2);
        // TODO: expectChangeNumberEvent()
    }
    */

    #[rstest]
    #[tokio::test]
    /// Signal Android: "full match"
    async fn full_match(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        assert_eq!(r1.id, r2.id);
        assert_eq!(storage.fetch_recipients().len(), 1);
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "e164 matches, e164 + aci provided"
    /// given(E164_A, PNI_A, null) → process(E164_A, null, ACI_A) → expect(E164_A, PNI_A, ACI_A)
    /// The existing `e164_matches_e164_provided_pni_changed` test does not cover this case.
    async fn e164_matches_e164_plus_aci_provided_pni_preserved(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);

        // process(E164_A, null, ACI_A) — ACI added; existing PNI must be preserved
        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), None, TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "pni matches, pni+aci provided, pni session"
    /// Same data outcome as the no-session variant; SSE check omitted (session API TODO).
    async fn pni_matches_pni_plus_aci_provided_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null, pniSession = true)
        // TODO: set up PNI session for pni once session API is available
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);

        // process(null, PNI_A, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(None, Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        // TODO: expectSessionSwitchoverEvent(E164_A)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "e164 and pni matches, all provided, new aci, existing pni session"
    /// Same data outcome as the no-session variant; SSE check omitted (session API TODO).
    async fn e164_and_pni_matches_all_provided_new_aci_existing_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null, pniSession = true)
        // TODO: set up PNI session for pni once session API is available
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);

        // process(E164_A, PNI_A, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        // TODO: expectSessionSwitchoverEvent(E164_A)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "pni matches, all provided, new e164 and aci, existing pni session"
    /// Same data outcome as the no-session variant; SSE check omitted (session API TODO).
    async fn pni_matches_all_provided_new_e164_and_aci_existing_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(null, PNI_A, null, pniSession = true)
        // TODO: set up PNI session for pni once session API is available
        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);

        // process(E164_A, PNI_A, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        // TODO: expectSessionSwitchoverEvent(E164_A)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "e164 and pni matches, all provided, existing pni session"
    /// Same data outcome as the no-session variant; SSE check omitted (session API TODO).
    async fn e164_and_pni_matches_all_provided_existing_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null, pniSession = true)
        // TODO: set up PNI session for pni once session API is available
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);

        // process(E164_A, PNI_A, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        // TODO: expectSessionSwitchoverEvent(E164_A)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "pni matches, all provided, existing pni session"
    /// Same data outcome as the no-session variant; SSE check omitted (session API TODO).
    async fn pni_matches_all_provided_existing_pni_session(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(null, PNI_A, null, pniSession = true)
        // TODO: set up PNI session for pni once session API is available
        let r1 = storage.merge_and_fetch_recipient(None, None, Some(pni(1)), TrustLevel::Uncertain);

        // process(E164_A, PNI_A, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Uncertain);
        verify(&r2, Id::Eq(r1.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        // TODO: expectSessionSwitchoverEvent(E164_A)
    }

    // -------------------------------------------------------------------------
    // Merge tests (continued) — change number variants
    // -------------------------------------------------------------------------

    /*
    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164+pni & e164+pni+aci, change number"
    async fn merge_e164_pni_and_e164_pni_aci_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null)
        // given(E164_B, PNI_B, ACI_A)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None, Some(pni(1)), TrustLevel::Uncertain);
        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), Some(pni(2)), TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, ACI_A)
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Certain);

        // ACI record (r2) changes number to e164_1+pni_1; r1 deleted (merged in)
        // expect(deleted) then expect(E164_A, PNI_A, ACI_A)
        verify(&r_out, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        assert!(storage.fetch_recipient_by_id(r1.id).is_none());
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectChangeNumberEvent()
        // TODO: expectThreadMergeEvent(E164_A)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164+pni & e164+aci, change number"
    async fn merge_e164_pni_and_e164_aci_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null)
        let r1 = storage.merge_and_fetch_recipient(Some(e164(1)), None,
            Some(pni(1)), TrustLevel::Uncertain);

        // given(E164_B, null, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None, TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(E164_A, PNI_A, ACI_A)
        let r_out = storage.merge_and_fetch_recipient(Some(e164(1)), Some(aci(1)), Some(pni(1)), TrustLevel::Certain);

        // ACI record (r2) changes number to e164_1; r1 deleted (merged in)
        // expect(deleted) then expect(E164_A, PNI_A, ACI_A)
        verify(&r_out, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        assert!(storage.fetch_recipient_by_id(r1.id).is_none());
        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectChangeNumberEvent()
        // TODO: expectThreadMergeEvent(E164_A)
    }

    #[rstest]
    #[tokio::test]
    /// Signal Android: "merge, e164+pni & e164+aci, pni+aci provided, change number"
    async fn merge_e164_pni_and_e164_aci_pni_aci_provided_change_number(storage: impl Future<Output = InMemoryDb>) {
        let (storage, _temp_dir) = storage.await;

        // given(E164_A, PNI_A, null)
        let _r = storage.merge_and_fetch_recipient(Some(e164(1)), None,
            Some(pni(1)), TrustLevel::Uncertain);

        // given(E164_B, null, ACI_A)
        let r2 = storage.merge_and_fetch_recipient(Some(e164(2)), Some(aci(1)), None, TrustLevel::Uncertain);
        assert_eq!(storage.fetch_recipients().len(), 2);

        // XXX: Pni doesn't get merged!
        // process(null, PNI_A, ACI_A)
        let r3 = storage.merge_and_fetch_recipient(None, Some(aci(1)), Some(pni(1)), TrustLevel::Certain);

        // PNI record (r1) carries e164_1 which follows; ACI record (r2) merges in
        // expect(E164_A, PNI_A, ACI_A)
        verify(&r3, Id::Eq(r2.id), Some(&e164(1)), Some(&pni(1)), Some(&aci(1)));

        assert_eq!(storage.fetch_recipients().len(), 1);
        // TODO: expectThreadMergeEvent(E164_A)
        // TODO: expectChangeNumberEvent()
    }
    */
}

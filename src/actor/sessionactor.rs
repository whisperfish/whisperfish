use crate::actor::FetchSession;

use crate::gui::StorageReady;
use crate::model::session::SessionModel;
use crate::sfos::SailfishApp;
use crate::store::{orm, Storage};

use actix::prelude::*;
use qmetaobject::*;

#[derive(Message)]
#[rtype(result = "()")]
#[allow(clippy::type_complexity)]
struct SessionsLoaded(
    Vec<(
        orm::Session,
        Vec<orm::Recipient>,
        orm::Message,
        Vec<orm::Attachment>,
        Vec<(orm::Receipt, orm::Recipient)>,
    )>,
);

#[derive(actix::Message)]
#[rtype(result = "()")]
// XXX this should be called *per message* instead of per session,
//     probably.
pub struct MarkSessionRead {
    pub sid: i32,
    pub already_unread: bool,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteSession {
    pub id: i32,
    pub idx: usize,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct LoadAllSessions;

pub struct SessionActor {
    inner: QObjectBox<SessionModel>,
    storage: Option<Storage>,
}

impl SessionActor {
    pub fn new(app: &mut SailfishApp) -> Self {
        let inner = QObjectBox::new(SessionModel::default());
        app.set_object_property("SessionModel".into(), inner.pinned());

        Self {
            inner,
            storage: None,
        }
    }
}

impl Actor for SessionActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());
    }
}

impl Handler<StorageReady> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        StorageReady(storage, _config): StorageReady,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage);
        log::trace!("SessionActor has a registered storage");

        ctx.notify(LoadAllSessions);
    }
}

impl Handler<SessionsLoaded> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        SessionsLoaded(sessions): SessionsLoaded,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let inner = self.inner.pinned();
        let mut inner = inner.borrow_mut();

        inner.handle_sessions_loaded(sessions);
    }
}

impl Handler<FetchSession> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        FetchSession { id: sid, mark_read }: FetchSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let storage = self.storage.as_ref().unwrap();
        let sess = storage.fetch_session_by_id(sid).expect("existing session");
        let message = storage
            .fetch_last_message_by_session_id(sid)
            .expect("> 0 messages per session");
        let receipts = storage.fetch_message_receipts(message.id);
        let attachments = storage.fetch_attachments_for_message(message.id);

        let group_members = if sess.is_group_v1() {
            let group = sess.unwrap_group_v1();
            storage
                .fetch_group_members_by_group_v1_id(&group.id)
                .into_iter()
                .map(|(_, r)| r)
                .collect()
        } else if sess.is_group_v2() {
            let group = sess.unwrap_group_v2();
            storage
                .fetch_group_members_by_group_v2_id(&group.id)
                .into_iter()
                .map(|(_, r)| r)
                .collect()
        } else {
            Vec::new()
        };

        self.inner.pinned().borrow_mut().handle_fetch_session(
            sess,
            group_members,
            message,
            attachments,
            receipts,
            mark_read,
        );
    }
}

impl Handler<MarkSessionRead> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionRead {
            sid,
            already_unread,
        }: MarkSessionRead,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_ref().unwrap().mark_session_read(sid);
        self.inner
            .pinned()
            .borrow_mut()
            .handle_mark_session_read(sid, already_unread);
    }
}

impl Handler<DeleteSession> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        DeleteSession { id, idx }: DeleteSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_ref().unwrap().delete_session(id);

        self.inner.pinned().borrow_mut().handle_delete_session(idx);
    }
}

impl Handler<LoadAllSessions> for SessionActor {
    type Result = ();

    /// Panics when storage is not yet set.
    fn handle(&mut self, _: LoadAllSessions, ctx: &mut Self::Context) {
        let session_actor = ctx.address();
        let storage = self.storage.clone().unwrap();

        actix::spawn(async move {
            let sessions = actix_threadpool::run(move || -> Result<_, failure::Error> {
                let sessions: Vec<orm::Session> = storage.fetch_sessions();
                let result = sessions
                    .into_iter()
                    .map(|session| {
                        // XXX maybe at some point we want a system where sessions don't necessarily
                        // contain a message.
                        let last_message = storage
                            .fetch_last_message_by_session_id(session.id)
                            .expect("a message in a session");
                        let last_message_receipts = storage.fetch_message_receipts(last_message.id);
                        let last_message_attachments =
                            storage.fetch_attachments_for_message(last_message.id);

                        let group_members = if session.is_group_v1() {
                            let group = session.unwrap_group_v1();
                            storage
                                .fetch_group_members_by_group_v1_id(&group.id)
                                .into_iter()
                                .map(|(_, r)| r)
                                .collect()
                        } else if session.is_group_v2() {
                            let group = session.unwrap_group_v2();
                            storage
                                .fetch_group_members_by_group_v2_id(&group.id)
                                .into_iter()
                                .map(|(_, r)| r)
                                .collect()
                        } else {
                            Vec::new()
                        };

                        (
                            session,
                            group_members,
                            last_message,
                            last_message_attachments,
                            last_message_receipts,
                        )
                    })
                    .collect();
                Ok(result)
            })
            .await
            .unwrap();
            // XXX handle error

            session_actor.send(SessionsLoaded(sessions)).await.unwrap();
            // XXX handle error
        });
    }
}

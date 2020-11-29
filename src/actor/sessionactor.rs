use crate::actor::FetchSession;

use crate::gui::StorageReady;
use crate::model::session::SessionModel;
use crate::sfos::SailfishApp;
use crate::store::{Session, Storage};

use actix::prelude::*;
use diesel::prelude::*;
use qmetaobject::*;

#[derive(Message)]
#[rtype(result = "()")]
struct SessionsLoaded(Vec<Session>);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MarkSessionRead {
    pub sess: Session,
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
        let sess = self.storage.as_ref().unwrap().fetch_session(sid);
        self.inner
            .pinned()
            .borrow_mut()
            .handle_fetch_session(sess.expect("FIXME No session returned!"), mark_read);
    }
}

impl Handler<MarkSessionRead> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionRead {
            sess,
            already_unread,
        }: MarkSessionRead,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_ref().unwrap().mark_session_read(&sess);
        self.inner
            .pinned()
            .borrow_mut()
            .handle_mark_session_read(sess, already_unread);
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
        let db = self.storage.clone().unwrap().db;

        Arbiter::spawn(async move {
            let sessions = actix_threadpool::run(move || -> Result<_, failure::Error> {
                let db = db
                    .lock()
                    .map_err(|_| failure::format_err!("Database mutex is poisoned."))?;
                use crate::schema::session::dsl::*;
                Ok(session.order_by(timestamp.desc()).load(&*db)?)
            })
            .await
            .unwrap();
            // XXX handle error

            session_actor.send(SessionsLoaded(sessions)).await.unwrap();
            // XXX handle error
        })
    }
}

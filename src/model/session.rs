use std::collections::HashMap;

use futures::prelude::*;

use crate::gui::StorageReady;
use crate::model::*;
use crate::sfos::SailfishApp;
use crate::store::{Session, Storage};

use actix::prelude::*;
use diesel::prelude::*;
use qmetaobject::*;

#[derive(QObject, Default)]
pub struct SessionModel {
    base: qt_base_class!(trait QAbstractListModel),
    actor: Option<Addr<SessionActor>>,

    content: Vec<Session>,
}

impl Session {
    fn section(&self) -> String {
        // XXX: stub
        "Section".into()
    }
}

define_model_roles! {
    enum SessionRoles for Session {
        ID(id):                                              "id",
        Source(source via QString::from):                    "source",
        IsGroup(is_group):                                   "isGroup",
        GroupName(group_name via qstring_from_option):       "groupName",
        GroupMembers(group_members via qstring_from_option): "groupMembers",
        Message(message via QString::from):                  "message",
        Section(fn section(&self) via QString::from):        "section",
        Timestamp(timestamp via qdatetime_from_i64):         "timestamp",
        Unread(unread):                                      "unread",
        Sent(sent):                                          "sent",
        Received(received):                                  "received",
        HasAttachment(has_attachment):                       "hasAttachment"
    }
}

impl QAbstractListModel for SessionModel {
    fn row_count(&self) -> i32 {
        self.content.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let role = SessionRoles::from(role);
        role.get(&self.content[index.row() as usize])
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        SessionRoles::role_names()
    }
}

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

    /// Panics when storage is not yet set.
    fn reload(&self, session_actor: Addr<SessionActor>) -> impl Future<Output=()> {
        let db = self.storage.clone().unwrap().db;

        async move {
            let sessions = actix_threadpool::run(move || -> Result<_, failure::Error> {
                let db = db.lock().map_err(|_| failure::format_err!("Database mutex is poisoned."))?;
                use crate::schema::session::dsl::*;
                Ok(session.order_by(timestamp.desc()).load(&*db)?)
            }).await.unwrap();
            // XXX handle error

            session_actor.send(SessionsLoaded(sessions)).await.unwrap();
            // XXX handle error
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct SessionsLoaded(Vec<Session>);

impl Handler<SessionsLoaded> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        SessionsLoaded(session): SessionsLoaded,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        use std::ops::DerefMut;

        let inner = self.inner.pinned();
        let mut inner = inner.borrow_mut();

        // XXX: maybe this should be called before even accessing the db?
        (inner.deref_mut().deref_mut() as &mut dyn QAbstractListModel).begin_reset_model();
        inner.content = session;
        (inner.deref_mut().deref_mut() as &mut dyn QAbstractListModel).end_reset_model();
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

        Arbiter::spawn(self.reload(ctx.address()));
    }
}

impl Actor for SessionActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());
    }
}

use actix::prelude::*;
use qmetaobject::QObject;
use qmetaobject::QPointer;

pub use whisperfish_model_macro::observing_model;

use crate::store::observer::Event;
use crate::store::observer::EventObserving;
use crate::store::observer::Interest;
use crate::store::{ActixEvent, Storage};

pub struct ModelContext<T: QObject + 'static> {
    pub(crate) storage: Storage,
    pub(crate) addr: Addr<ObservingModelActor<T>>,
}

impl<T: QObject + 'static> Clone for ModelContext<T> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            addr: self.addr.clone(),
        }
    }
}

impl<T: QObject + 'static> ModelContext<T> {
    pub fn storage(&self) -> Storage {
        self.storage.clone()
    }
    pub fn addr(&self) -> Addr<ObservingModelActor<T>> {
        self.addr.clone()
    }
}

/// An actor that accompanies the `ObservingModel`, responsible to dispatch events to the contained
/// model.
///
/// The contained model is a weak pointer, such that the actor will stop when the model goes out of
/// scope.
pub struct ObservingModelActor<T: QObject> {
    pub(super) model: QPointer<T>,
    pub(super) storage: Storage,
}

impl<T: QObject + 'static> actix::Actor for ObservingModelActor<T> {
    type Context = actix::Context<Self>;
}

impl<T: QObject + 'static> actix::Handler<ActixEvent> for ObservingModelActor<T>
where
    T: EventObserving<Context = ModelContext<T>>,
{
    type Result = Vec<Interest>;

    fn handle(&mut self, event: ActixEvent, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace_span!(
            "ObservingModelActor",
            T = std::any::type_name::<T>(),
            ?event
        )
        .in_scope(|| {
            match self.model.as_pinned() {
                Some(model) => {
                    let mut model = model.borrow_mut();
                    let ctx = ModelContext {
                        storage: self.storage.clone(),
                        addr: ctx.address(),
                    };
                    model.observe(ctx, Event::from(event));
                    model.interests()
                }
                None => {
                    // In principle, the actor should have gotten stopped when the model got dropped,
                    // because the actor's only strong reference is contained in the ObservingModel.
                    tracing::debug!("Model got dropped, stopping actor execution.");
                    // XXX What is the difference between stop and terminate?
                    ctx.stop();
                    Vec::new()
                }
            }
        })
    }
}

pub struct ObservingModelRegistration<T: QObject + 'static> {
    pub(crate) actor: actix::Addr<ObservingModelActor<T>>,
    pub(crate) observer_handle: crate::store::observer::ObserverHandle,
}

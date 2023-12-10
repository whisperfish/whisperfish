use actix::prelude::*;
use qmetaobject::{QObject, QObjectBox};

use crate::store::observer::Event;
use crate::store::observer::EventObserving;
use crate::store::observer::Interest;
use crate::store::Storage;

#[macro_export]
macro_rules! observing_model {
    ($vis:vis struct $model:ident($encapsulated:ty) {
        $($property:ident: $t:ty; READ $getter:ident $(WRITE $setter:ident)?),* $(,)?
    } $(
        WITH OPTIONAL PROPERTIES FROM $field:ident WITH ROLE $role:ident {
            $($opt_property:ident $role_variant:ident),* $(,)?
        }
    )?) => {
        #[derive(QObject)]
        $vis struct $model {
            base: qt_base_class!(trait QObject),
            inner: std::sync::Arc<qmetaobject::QObjectBox<$encapsulated>>,
            actor: Option<actix::Addr<ObservingModelActor<$encapsulated>>>,
            observer_handle: Option<$crate::store::observer::ObserverHandle>,

            app: qt_property!(QPointer<$crate::gui::AppState>; WRITE set_app),

            reinit: qt_method!(fn(&mut self)),

            $(
                #[allow(unused)]
                $property: qt_property!($t; READ $getter $(WRITE $setter)? NOTIFY something_changed),
            )*

            $(
            $(
                #[allow(unused)]
                $opt_property: qt_property!(QVariant; READ $opt_property NOTIFY something_changed),
            )*
            )?

            something_changed: qt_signal!(),
        }

        impl Default for $model {
            fn default() -> Self {
                let inner = std::sync::Arc::<qmetaobject::QObjectBox::<$encapsulated>>::default();

                Self {
                    base: Default::default(),
                    app: Default::default(),
                    inner,
                    actor: None,
                    observer_handle: None,
                    reinit: Default::default(),
                    $( $property: Default::default(), )*
                    $($( $opt_property: Default::default(), )*)?
                    something_changed: Default::default(),
                }
            }
        }

        impl $model {
            #[qmeta_async::with_executor]
            fn set_app(&mut self, app: QPointer<$crate::gui::AppState>) {
                self.app = app;
                self.reinit();
            }

            fn reinit(&mut self) {
                use actix::prelude::*;
                if let Some(app) = self.app.as_pinned() {
                    if let Some(mut storage) = app.borrow().storage.borrow().clone() {
                        let actor = ObservingModelActor {
                            model: std::sync::Arc::downgrade(&self.inner),
                            storage: storage.clone(),
                        }
                        .start();

                        let subscriber = actor.downgrade().recipient();
                        let ctx = $crate::model::active_model::ModelContext {
                            storage: storage.clone(),
                            addr: actor.clone(),
                        };
                        self.actor = Some(actor);
                        self.inner.pinned().borrow_mut().init(ctx);
                        let handle = storage.register_observer($crate::store::observer::EventObserving::interests(&*self.inner.pinned().borrow()), subscriber);
                        self.observer_handle = Some(handle);

                        self.something_changed();
                    }
                }
            }

            $(
            $(
                fn $opt_property(&self) -> qmetaobject::QVariant {
                    match self.inner.pinned().borrow().$field.as_ref() {
                        Some(x) => {
                            ($role::$role_variant).get(x)
                        }
                        None => qmetaobject::QVariant::default()
                    }
                }
            )*
            )?
            $(
                fn $getter(&self) -> $t {
                    self.inner.pinned().borrow().$getter()
                }

                $(
                #[qmeta_async::with_executor]
                fn $setter(&mut self, v: $t) {
                    let storage = self.app.as_pinned().and_then(|app| app.borrow().storage.borrow().clone());
                    let addr = self.actor.clone();
                    let ctx = storage.clone().zip(addr).map(|(storage, addr)| {
                        $crate::model::active_model::ModelContext {
                            storage,
                            addr,
                        }
                    });
                    self.inner.pinned().borrow_mut().$setter(
                        ctx,
                        v,
                    );
                    if let (Some(mut storage), Some(handle)) = (storage, self.observer_handle) {
                        storage.update_interests(handle, self.inner.pinned().borrow().interests());
                    }
                    self.something_changed();
                }
                )?
            )*
        }
    };
}

pub struct ModelContext<T: QObject + 'static> {
    pub(crate) storage: Storage,
    pub(crate) addr: Addr<ObservingModelActor<T>>,
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
    pub(super) model: std::sync::Weak<QObjectBox<T>>,
    pub(super) storage: Storage,
}

impl<T: QObject + 'static> actix::Actor for ObservingModelActor<T> {
    type Context = actix::Context<Self>;
}

impl<T: QObject + 'static> actix::Handler<Event> for ObservingModelActor<T>
where
    T: EventObserving<Context = ModelContext<T>>,
{
    type Result = Vec<Interest>;

    fn handle(&mut self, event: Event, ctx: &mut Self::Context) -> Self::Result {
        match self.model.upgrade() {
            Some(model) => {
                let model = model.pinned();
                let mut model = model.borrow_mut();
                let ctx = ModelContext {
                    storage: self.storage.clone(),
                    addr: ctx.address(),
                };
                model.observe(ctx, event);
                model.interests()
            }
            None => {
                // In principle, the actor should have gotten stopped when the model got dropped,
                // because the actor's only strong reference is contained in the ObservingModel.
                log::debug!("Model got dropped, stopping actor execution.");
                // XXX What is the difference between stop and terminate?
                ctx.stop();
                Vec::new()
            }
        }
    }
}

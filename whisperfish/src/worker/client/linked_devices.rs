use super::*;
use qmeta_async::with_executor;
use std::convert::TryInto;

#[derive(Message)]
#[rtype(result = "()")]
pub struct ReloadLinkedDevices;

#[derive(Message)]
#[rtype(result = "()")]
pub struct LinkDevice {
    pub tsurl: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UnlinkDevice {
    pub id: i64,
}

// methods called from Qt
impl ClientWorker {
    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn link_device(&self, tsurl: String) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor.send(LinkDevice { tsurl }).await {
                tracing::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn unlink_device(&self, id: i64) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor.send(UnlinkDevice { id }).await {
                tracing::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn reload_linked_devices(&self) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor.send(ReloadLinkedDevices).await {
                tracing::error!("{:?}", e);
            }
        });
    }
}

impl Handler<ReloadLinkedDevices> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _: ReloadLinkedDevices, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("handle(ReloadLinkedDevices)");

        let mut service = self.authenticated_service();

        Box::pin(
            // Without `async move`, service would be borrowed instead of encapsulated in a Future.
            async move { service.devices().await }.into_actor(self).map(
                move |result, act, _ctx| {
                    match result {
                        Err(e) => {
                            // XXX show error
                            tracing::error!("Refresh linked devices failed: {}", e);
                        }
                        Ok(devices) => {
                            tracing::trace!("Successfully refreshed linked devices: {:?}", devices);
                            // A bunch bindings because of scope
                            let client_worker = act.inner.pinned();
                            let client_worker = client_worker.borrow_mut();
                            let device_model =
                                client_worker.device_model.as_ref().unwrap().pinned();
                            device_model.borrow_mut().set_devices(devices);
                        }
                    }
                },
            ),
        )
    }
}

impl Handler<LinkDevice> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        LinkDevice { tsurl }: LinkDevice,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        tracing::trace!("handle(LinkDevice)");

        let service = self.authenticated_service();
        let credentials = self.credentials.clone().unwrap();
        let store = self.storage.clone().unwrap();
        let profile_key: Option<[u8; 32]> = store
            .fetch_self_recipient_profile_key()
            .and_then(|key| key.try_into().ok());
        let mut account_manager = AccountManager::new(service, profile_key.map(ProfileKey::create));
        let master_key = store.fetch_master_key();

        Box::pin(
            // Without `async move`, service would be borrowed instead of encapsulated in a Future.
            async move {
                let url = tsurl.parse()?;
                Ok::<_, anyhow::Error>(
                    account_manager
                        .link_device(
                            url,
                            &store.aci_storage(),
                            &store.pni_storage(),
                            credentials,
                            master_key,
                        )
                        .await?,
                )
            }
            .into_actor(self)
            .map(move |result, _act, ctx| {
                match result {
                    Err(e) => {
                        // XXX show error
                        tracing::error!("Linking device failed: {}", e);
                    }
                    Ok(()) => {
                        tracing::trace!("Linked device succesfully");
                        // A bunch bindings because of scope
                        ctx.notify(ReloadLinkedDevices);
                    }
                }
            }),
        )
    }
}

impl Handler<UnlinkDevice> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        UnlinkDevice { id }: UnlinkDevice,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        tracing::trace!("handle(UnlinkDevice)");

        let mut service = self.authenticated_service();

        Box::pin(
            // Without `async move`, service would be borrowed instead of encapsulated in a Future.
            async move { service.unlink_device(id).await }
                .into_actor(self)
                .map(move |result, _act, ctx| {
                    match result {
                        Err(e) => {
                            // XXX show error in UI
                            tracing::error!("Delete linked device failed: {}", e);
                        }
                        Ok(()) => {
                            tracing::trace!("Successfully unlinked device");
                            ctx.notify(ReloadLinkedDevices);
                        }
                    }
                }),
        )
    }
}

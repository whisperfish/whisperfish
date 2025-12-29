#![allow(non_snake_case)]
use std::sync::Arc;

use image::Rgba;
use qmetaobject::prelude::*;
use rustlegraph::{VizualizationParameters, Vizualizer};
use whisperfish_store::orm;

#[derive(Default, QObject)]
pub struct RustleGraph {
    base: qt_base_class!(trait QObject),
    app: qt_property!(QPointer<crate::gui::AppState>; WRITE set_app),
    attachmentId: qt_property!(i32; READ get_attachment_id WRITE set_attachment_id),

    width: qt_property!(u32; WRITE set_width),
    height: qt_property!(u32; WRITE set_height),

    duration: qt_property!(f64; READ get_duration NOTIFY duration_updated),

    timestamp: qt_property!(f64; WRITE set_time),

    imageId: qt_property!(QString; READ get_image_id NOTIFY image_updated),

    pastColor: qt_property!(QColor; WRITE set_past_color),
    futureColor: qt_property!(QColor; WRITE set_future_color),

    image_updated: qt_signal!(),
    duration_updated: qt_signal!(),

    vizualizer: Option<Arc<Vizualizer>>,
}

fn qcolor_to_image(c: QColor) -> Rgba<u8> {
    let (r, g, b, a) = c.get_rgba();
    Rgba([r as u8, g as u8, b as u8, a as u8])
}

impl RustleGraph {
    #[qmeta_async::with_executor]
    #[tracing::instrument(skip(self, app))]
    fn set_app(&mut self, app: QPointer<crate::gui::AppState>) {
        self.app = app;
        self.reinit();
    }

    fn vizualizer_params(&self) -> VizualizationParameters {
        VizualizationParameters {
            width: self.width,
            height: self.height,
            past_color: qcolor_to_image(self.pastColor),
            future_color: qcolor_to_image(self.futureColor),
        }
    }

    #[qmeta_async::with_executor]
    fn set_past_color(&mut self, color: QColor) {
        self.pastColor = color;
        self.reinit();
    }

    #[qmeta_async::with_executor]
    fn set_future_color(&mut self, color: QColor) {
        self.futureColor = color;
        self.reinit();
    }

    #[qmeta_async::with_executor]
    fn set_width(&mut self, width: u32) {
        self.width = width;
        self.reinit();
    }

    #[qmeta_async::with_executor]
    fn set_height(&mut self, height: u32) {
        self.height = height;
        self.reinit();
    }

    #[qmeta_async::with_executor]
    fn set_time(&mut self, time: f64) {
        self.timestamp = time;
        // Don't reinitialize here.
        self.image_updated();
    }

    fn get_duration(&self) -> f64 {
        if let Some(viz) = self.vizualizer.as_ref() {
            let time = viz.time();
            time.seconds as f64 + time.frac
        } else {
            0.
        }
    }

    async fn load_vizualizer(this: QPointer<Self>, attachment: orm::Attachment) {
        if !attachment.is_voice_note {
            tracing::warn!("Attachment is not a voice note.");
            /* no-op */
            return;
        }

        let Some(path) = attachment.absolute_attachment_path() else {
            tracing::warn!("attachment without known absolute path");
            return;
        };

        let vizualizer = {
            // Fetch params, don't hold across await point.
            let params = {
                let Some(this) = this.as_pinned() else {
                    tracing::debug!("object dropped, aborting load");
                    return;
                };
                // This kind of calls and checks should probably be guarded automatically/generically by the model
                // macro...
                if this.borrow().attachmentId != attachment.id {
                    tracing::debug!("attachment id changed while loading, aborting load");
                    return;
                }

                this.borrow().vizualizer_params()
            };

            tracing::debug!(
                "Generating a RustleGraph of {}x{}",
                params.width,
                params.height
            );
            // We need those to be 'static, so we clone and move into the threadpool.
            let content_type = attachment.content_type.clone();
            let filename = std::path::PathBuf::from(path.into_owned());
            let vizualizer = tokio::task::spawn_blocking(move || {
                Vizualizer::from_file(params, Some(&content_type), &filename)
            })
            .await
            .expect("threadpool");
            match vizualizer {
                Ok(vizualizer) => vizualizer,
                Err(e) => {
                    tracing::error!("Vizualization failed: {}", e);
                    return;
                }
            }
        };

        let Some(this) = this.as_pinned() else {
            tracing::debug!("object dropped, aborting load");
            return;
        };
        let mut this = this.borrow_mut();

        let vizualizer = Arc::new(vizualizer);

        // Move the owned reference into self.
        let vizualizer = Arc::downgrade(this.vizualizer.insert(vizualizer));

        let Some(app) = this.app.as_pinned() else {
            return;
        };
        let app = app.borrow();

        // Put the vizualizer in the map
        let id = this.image_id();
        let _old = app.rustlegraphs.borrow_mut().insert(id, vizualizer);
        if _old.is_some() {
            tracing::info!("Replaced an old Rustlegraph; probably doing double work here");
        }

        this.image_updated();
        this.duration_updated();
    }

    fn reinit(&mut self) {
        if self.attachmentId == 0 {
            return;
        }
        if self.width == 0 || self.height == 0 || self.width > 10000 || self.height > 10000 {
            return;
        }

        if let Some(app) = self.app.as_pinned() {
            let app = app.borrow();

            // Cleanup the hashmap a bit.
            if let Some(cleanup) = self.vizualizer.take() {
                drop(cleanup);
                app.rustlegraphs.borrow_mut().retain(|k, v| {
                    if v.strong_count() == 0 {
                        tracing::trace!("Removing RustleGraph {} from cache", k);
                    }
                    v.strong_count() > 0
                });
            }

            self.image_updated();
            self.duration_updated();

            // Generate the vizualizer if we have all the data
            if let Some(storage) = app.storage.borrow().clone() {
                if let Some(attachment) = storage.fetch_attachment(self.attachmentId) {
                    let this = QPointer::from(&*self);
                    actix::spawn(Self::load_vizualizer(this, attachment));
                }
            }
        }
    }

    fn get_attachment_id(&self) -> i32 {
        self.attachmentId
    }

    #[qmeta_async::with_executor]
    fn set_attachment_id(&mut self, id: i32) {
        self.attachmentId = id;
        self.reinit();
    }

    fn image_id(&self) -> String {
        if self.vizualizer.is_some() {
            let p = qcolor_to_image(self.pastColor);
            let f = qcolor_to_image(self.futureColor);
            format!(
                "{}:{}x{}:{:?}-{:?}",
                self.attachmentId, self.width, self.height, p, f
            )
        } else {
            String::new()
        }
    }

    fn get_image_id(&self) -> QString {
        if self.vizualizer.is_some() {
            format!("{}:{}", self.image_id(), self.timestamp).into()
        } else {
            QString::default()
        }
    }
}

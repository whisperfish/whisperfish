#![allow(non_snake_case)]
use std::sync::Arc;

use image::Rgba;
use qmetaobject::prelude::*;
use rustlegraph::{VizualizationParameters, Vizualizer};

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

    fn set_past_color(&mut self, color: QColor) {
        self.pastColor = color;
        self.reinit();
    }

    fn set_future_color(&mut self, color: QColor) {
        self.futureColor = color;
        self.reinit();
    }

    fn set_width(&mut self, width: u32) {
        self.width = width;
        self.reinit();
    }

    fn set_height(&mut self, height: u32) {
        self.height = height;
        self.reinit();
    }

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

            // Generate the vizualizer if we have all the data
            if let Some(storage) = app.storage.borrow().clone() {
                if let Some(att) = storage.fetch_attachment(self.attachmentId) {
                    if !att.is_voice_note {
                        tracing::warn!("Attachment is not a voice note.");
                        self.image_updated();
                        self.duration_updated();
                        return;
                    }
                    if let Some(path) = att.absolute_attachment_path() {
                        tracing::debug!(
                            "Generating a RustleGraph of {}x{}",
                            self.width,
                            self.height
                        );
                        let viz = Vizualizer::from_file(
                            self.vizualizer_params(),
                            Some(&att.content_type),
                            std::path::Path::new(path.as_ref()),
                        );
                        match viz {
                            Ok(viz) => self.vizualizer = Some(Arc::new(viz)),
                            Err(e) => {
                                tracing::error!("Vizualization failed: {}", e);
                            }
                        }
                    }
                }
            }

            // Put the vizualizer in the map
            if let Some(v) = &self.vizualizer {
                let id = self.image_id();
                let _old = app.rustlegraphs.borrow_mut().insert(id, Arc::downgrade(v));
                if _old.is_some() {
                    tracing::info!("Replaced an old Rustlegraph; probably doing double work here");
                }
            }
            self.image_updated();
            self.duration_updated();
        }
    }

    fn get_attachment_id(&self) -> i32 {
        self.attachmentId
    }

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

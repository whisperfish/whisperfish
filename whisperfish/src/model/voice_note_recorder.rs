#![allow(non_snake_case)]
use qmetaobject::prelude::*;

#[derive(Default, QObject)]
pub struct VoiceNoteRecorder {
    base: qt_base_class!(trait QObject),
    app: qt_property!(QPointer<crate::gui::AppState>; WRITE set_app),

    duration: qt_property!(f64; READ get_duration NOTIFY duration_updated),
    recording: qt_property!(bool; READ get_recording NOTIFY recording_updated),

    recording_updated: qt_signal!(),

    start: qt_method!(fn(&mut self)),
    stop: qt_method!(fn(&mut self)),
    reset: qt_method!(fn(&mut self)),
}

impl VoiceNoteRecorder {
    #[qmeta_async::with_executor]
    #[tracing::instrument(skip(self, app))]
    fn set_app(&mut self, app: QPointer<crate::gui::AppState>) {
        self.app = app;
        // self.reinit();
    }

    fn get_duration(&self) -> f64 {
        todo!()
    }

    fn start(&mut self) {
        todo!()
    }

    fn stop(&mut self) {
        todo!()
    }

    fn reset(&mut self) {
        self.stop();

        todo!();
        // self.duration_updated();
    }

    fn get_recording(&self) -> bool {
        false
    }
}

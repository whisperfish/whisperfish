#![allow(non_snake_case)]
use gstreamer::{self as gst, prelude::*};
use qmetaobject::prelude::*;

#[derive(Default, QObject)]
pub struct VoiceNoteRecorder {
    base: qt_base_class!(trait QObject),
    app: qt_property!(QPointer<crate::gui::AppState>; WRITE set_app),

    isRecording: qt_property!(bool; READ get_recording NOTIFY recording_updated),
    file: qt_property!(String; READ get_file NOTIFY file_changed),

    recording_updated: qt_signal!(),
    file_changed: qt_signal!(),

    start: qt_method!(fn(&mut self, path: String)),
    stop: qt_method!(fn(&mut self) -> String),
    reset: qt_method!(fn(&mut self)),

    handle: Option<Recording>,
}

struct Recording {
    main_loop: glib::MainLoop,
    pipeline: gst::Pipeline,
    filename: String,
}

impl Recording {
    fn stop(self) -> String {
        self.pipeline
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state.");
        self.main_loop.quit();
        self.filename
    }
}

#[tracing::instrument]
fn start_recording(filename: String) -> Recording {
    tracing::trace!("initializing recording");
    let dir = std::path::Path::new(&filename).parent().unwrap();
    if !dir.exists() {
        let directory = dir.display();
        tracing::info!(%directory, "creating for recording");
        std::fs::create_dir_all(dir).unwrap();
    }

    let main_loop = glib::MainLoop::new(None, false);
    let pipeline = gst::Pipeline::with_name("test-pipeline");
    let main_loop_clone = main_loop.clone();
    let pipeline_clone = pipeline.clone();

    let filename_clone = filename.clone();
    std::thread::Builder::new()
        .name(format!("recording {}", filename))
        .spawn(move || {
            // Create PlayBin element
            let pulsesrc = gst::ElementFactory::make("pulsesrc")
                .name("pulsesrc")
                .property("client-name", "Whisperfish voice note recorder")
                .build()
                .expect("create pulsesrc element");

            let audio_convert = gst::ElementFactory::make("audioconvert")
                .name("audio_convert")
                .build()
                .unwrap();

            // TODO: Currently, rustlegraph can't render Opus,
            //       because Symphonia doesn't decode it yet: https://github.com/pdeljanov/Symphonia/issues/8
            //       So we use Vorbis for now.
            // let opusenc = gst::ElementFactory::make("opusenc")
            //     .name("opusenc")
            //     .build()
            //     .unwrap();
            // let enc = opusenc;
            let vorbisenc = gst::ElementFactory::make("vorbisenc")
                .name("vorbisenc")
                .build()
                .unwrap();
            let enc = vorbisenc;

            let oggmux = gst::ElementFactory::make("oggmux")
                .name("oggmux")
                .build()
                .unwrap();

            let filesink = gst::ElementFactory::make("filesink")
                .name("filesink")
                .property("location", &filename_clone)
                .build()
                .unwrap();

            pipeline
                .add_many([&pulsesrc, &audio_convert, &enc, &oggmux, &filesink])
                .unwrap();

            gst::Element::link_many([&pulsesrc, &audio_convert, &enc, &oggmux, &filesink]).unwrap();

            pipeline
                .set_state(gst::State::Playing)
                .expect("Unable to set the pipeline to the `Playing` state.");
            tracing::info!("recording loop started");

            main_loop.run();
            tracing::info!("recording loop stopped");
        })
        .unwrap();

    Recording {
        main_loop: main_loop_clone,
        pipeline: pipeline_clone,
        filename,
    }
}

impl VoiceNoteRecorder {
    #[qmeta_async::with_executor]
    #[tracing::instrument(skip(self, app))]
    fn set_app(&mut self, app: QPointer<crate::gui::AppState>) {
        self.app = app;
    }

    fn get_file(&self) -> String {
        if let Some(handle) = &self.handle {
            return handle.filename.clone();
        }

        "".to_string()
    }

    fn start(&mut self, filename: String) {
        self.handle = Some(start_recording(filename));
        self.file_changed();
        self.recording_updated();
    }

    fn stop(&mut self) -> String {
        if let Some(handle) = self.handle.take() {
            let f = handle.stop();
            self.handle = None;
            self.recording_updated();
            return f;
        }

        "".to_string()
    }

    fn reset(&mut self) {
        if let Some(handle) = self.handle.take() {
            let f = handle.stop();
            self.handle = None;
            self.recording_updated();
            std::fs::remove_file(f).unwrap();
        }
    }

    fn get_recording(&self) -> bool {
        self.handle.is_some()
    }
}

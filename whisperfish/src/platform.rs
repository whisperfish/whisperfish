#[cfg(feature = "sailfish")]
mod sailfish_inner {
    pub use sailors::sailfishapp::{QQmlEngine, QmlApp};
}

#[cfg(not(feature = "sailfish"))]
mod sailfish_inner {
    use qmetaobject::qttypes::{QString, QVariant};

    // This is a noop implementation of QmlApp that will crash
    // when you try to launch it.
    // Enable the `sailfish` feature to build the app correctly

    pub struct QmlApp;
    pub struct QQmlEngine;

    impl QmlApp {
        pub fn application(_app: String) -> Self {
            QmlApp
        }

        pub fn path_to(path: String) -> String {
            path
        }

        pub fn set_property(&mut self, _name: QString, _value: QVariant) {}
        pub fn set_object_property<T>(&mut self, _name: QString, _item: T) {}
        pub fn set_title(&mut self, _title: QString) {}
        pub fn set_application_version(&mut self, _version: QString) {}
        pub fn install_default_translator(&mut self) -> Option<()> {
            Some(())
        }
        pub fn set_quit_on_last_window_closed(&mut self, _quit: bool) {}
        pub fn promote_gui_app_to_qml_context(&mut self, _name: QString) {}
        pub fn set_source(&mut self, _source: String) {}
        pub fn show_full_screen(&mut self) {}
        pub fn exec(self) {
            panic!(
                "Whisperfish has been compiled in development mode. The application will not work. Please compile Whisperfish with the `sailfish` feature to have a working application."
            );
        }
        pub fn engine(&mut self) -> &mut QQmlEngine {
            panic!(
                "Whisperfish has been compiled in development mode. The application will not work. Please compile Whisperfish with the `sailfish` feature to have a working application."
            );
        }
    }
}

pub use self::sailfish_inner::*;

#[cfg(feature = "harbour")]
mod harbour_inner {
    pub struct MayExit;

    impl MayExit {
        pub fn new() -> Self {
            MayExit
        }

        pub fn may_exit(&self) -> bool {
            true
        }

        pub fn set_may_exit(&mut self, _may_exit: bool) {}
    }

    pub fn is_harbour() -> bool {
        true
    }
}

#[cfg(not(feature = "harbour"))]
mod harbour_inner {
    pub struct MayExit {
        may_exit: bool,
    }

    impl MayExit {
        pub fn new() -> Self {
            MayExit { may_exit: true }
        }

        pub fn may_exit(&self) -> bool {
            self.may_exit
        }

        pub fn set_may_exit(&mut self, may_exit: bool) {
            self.may_exit = may_exit;
        }
    }

    pub fn is_harbour() -> bool {
        false
    }
}

pub use self::harbour_inner::{MayExit, is_harbour};

impl Default for MayExit {
    fn default() -> Self {
        MayExit::new()
    }
}

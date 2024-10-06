use qmetaobject::{log::*, prelude::*, QMessageLogContext, QtMsgType};
use tracing::Level;

static QLEVEL: &[Level] = &[
    Level::DEBUG, // 0 = QDebug
    Level::WARN,  // 1 = QWarning
    Level::ERROR, // 2 = QCritical
    Level::ERROR, // 3 = QFatal
    Level::INFO,  // 4 = QInfo
    Level::ERROR, // 5 = QSystem
    Level::ERROR, // 6 = _
];

const FILE_START: &str = "file:///usr/share/harbour-whisperfish/";

#[no_mangle]
pub extern "C" fn log_qt(msg_type: QtMsgType, msg_context: &QMessageLogContext, msg: &QString) {
    // QML may have prepended the message with the file information (so shorten it a bit),
    // or QMessageLogContext may provide it to us.
    let mut new_msg = msg.to_string();

    if new_msg.contains(FILE_START) {
        new_msg = new_msg.replace(FILE_START, "");
    } else if !msg_context.file().is_empty() {
        new_msg = format!(
            "{}:{}:{}(): {}",
            msg_context.file().replace(FILE_START, ""),
            msg_context.line(),
            msg_context.function(),
            msg
        );
    }

    let level = QLEVEL.get(msg_type as usize).unwrap_or(&QLEVEL[6]);
    match *level {
        Level::TRACE => tracing::trace!("{new_msg}"),
        Level::DEBUG => tracing::debug!("{new_msg}"),
        Level::INFO => tracing::info!("{new_msg}"),
        Level::WARN => tracing::warn!("{new_msg}"),
        Level::ERROR => tracing::error!("{new_msg}"),
    }
}

pub fn enable() -> QtMessageHandler {
    install_message_handler(Some(log_qt))
}

pub fn disable() -> QtMessageHandler {
    install_message_handler(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qml_to_rust_logging() {
        let handler_a = enable();
        assert!(handler_a.is_some());

        let handler_b = disable();
        assert!(handler_b.is_some());

        assert_ne!(handler_a.unwrap() as usize, handler_b.unwrap() as usize);

        let handler_b = enable();
        assert!(handler_b.is_some());

        assert_eq!(handler_a.unwrap() as usize, handler_b.unwrap() as usize);
    }
}

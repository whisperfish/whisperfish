use crate::platform::QQmlEngine;
use cpp::cpp;
use qttypes::QString;

pub fn install(app: &mut QQmlEngine) {
    cpp!(unsafe [app as "QQmlEngine *"] {
        app->addImageProvider(QLatin1String("blurhash"), new BlurhashImageProvider);
    });
}

cpp! {{
    #include <QtQuick/QQuickImageProvider>

    class BlurhashImageProvider : public QQuickImageProvider
    {
    public:
        BlurhashImageProvider()
                   : QQuickImageProvider(QQuickImageProvider::Image)
        {
        }

        QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override
        {
            int width = 100;
            int height = 100;

            if (size)
               *size = QSize(width, height);

            QImage img(requestedSize.width() > 0 ? requestedSize.width() : width,
                           requestedSize.height() > 0 ? requestedSize.height() : height,
                           QImage::Format::Format_RGBX8888);
            uchar *buf = img.bits();

            #if (QT_VERSION >= QT_VERSION_CHECK(5, 10, 0))
            size_t size_in_bytes = img.sizeInBytes();
            #else
            size_t size_in_bytes = img.byteCount();
            #endif

            width = img.width();
            height = img.height();

            int ret = rust!(WF_decode_blurhash [
                id : &QString as "const QString &",
                buf : *mut u8 as "uchar *",
                width : u32 as "int",
                height : u32 as "int",
                size_in_bytes : usize as "size_t"
            ] -> i32 as "int" {
                let id = id.to_string();
                // XXX We might want some *real* error handling at some point.
                if id == "null" {
                    tracing::warn!("Received 'null' as blurhash ID. Returning transparent image.");
                    return -1;
                }
                let id = match percent_encoding::percent_decode_str(&id).decode_utf8() {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::warn!("Could not percent-decode {} ({}). Continuing with raw string.", id, e);
                        std::borrow::Cow::Borrowed(&id as &str)
                    }
                };

                let slice = unsafe { std::slice::from_raw_parts_mut(buf, size_in_bytes) };

                if let Err(e) = blurhash::decode_into(slice, id.as_ref(), width, height, 1.0) {
                    tracing::warn!("Could not decode blurhash: {}", e);
                    return -2;
                }
                0
            });

            if (ret != 0) {
                img.fill(Qt::transparent);
            }

            return img;
        }
    };
} }

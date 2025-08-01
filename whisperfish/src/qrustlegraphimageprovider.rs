use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Weak};

use crate::platform::QQmlEngine;
use cpp::cpp;
use qttypes::QString;

type VizualizerMap = Rc<RefCell<HashMap<String, Weak<rustlegraph::Vizualizer>>>>;

pub fn install(app: &mut QQmlEngine, vizualizers: VizualizerMap) {
    let vizualizers: *mut VizualizerMap = Box::leak(Box::new(vizualizers));
    cpp!(unsafe [app as "QQmlEngine *", vizualizers as "void *"] {
        app->addImageProvider(QLatin1String("rustlegraph"), new RustleGraphImageProvider(vizualizers));
    });
}

cpp! {{
    #include <QtQuick/QQuickImageProvider>

    class RustleGraphImageProvider : public QQuickImageProvider
    {
        void *ctx;
    public:
        RustleGraphImageProvider(void *ctx)
                   : QQuickImageProvider(QQuickImageProvider::Image),
                     ctx(ctx)
        {
        }
        RustleGraphImageProvider(RustleGraphImageProvider &other) = delete;

        ~RustleGraphImageProvider() {
            rust!(WF_rustlegraph_destructor [
                ctx: *mut VizualizerMap as "void *"
            ] {
                // Explicit drop because of must_use
                drop(Box::<VizualizerMap>::from_raw(ctx));
            });
        }

        QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override
        {
            int width = 100;
            int height = 40;

            int *widthp = &width;
            int *heightp = &height;

            int ret;

            ret = rust!(WF_rustlegraph_compute_dims [
                id : &QString as "const QString &",
                widthp : &mut i32 as "int *",
                heightp : &mut i32 as "int *"
            ] -> i32 as "int" {
                let id = id.to_string();
                if id.is_empty() {
                    tracing::warn!("Received empty RustleGraph ID. Returning transparent image.");
                    return -1;
                }

                let mut id = id.split(':');
                id.next().unwrap();
                let dims = id.next().unwrap();
                let mut dims = dims.split('x');
                *widthp = dims.next().unwrap().parse().unwrap();
                *heightp = dims.next().unwrap().parse().unwrap();

                0
            });

            if (ret != 0) {
                return QImage();
            }

            width = requestedSize.width() > 0 ? requestedSize.width() : width;
            height = requestedSize.height() > 0 ? requestedSize.height() : height;

            QImage img(width, height, QImage::Format::Format_RGBA8888);

            if (size)
               *size = QSize(width, height);

            img.fill(0);
            uchar *buf = img.bits();

            #if (QT_VERSION >= QT_VERSION_CHECK(5, 10, 0))
            size_t size_in_bytes = img.sizeInBytes();
            #else
            size_t size_in_bytes = img.byteCount();
            #endif

            ret = rust!(WF_inject_rustlegraph [
                id : &QString as "const QString &",
                buf : *mut u8 as "uchar *",
                width : u32 as "int",
                height : u32 as "int",
                size_in_bytes : usize as "size_t",
                ctx : *mut VizualizerMap as "void *"
            ] -> i32 as "int" {
                let id = id.to_string();
                if id.is_empty() {
                    tracing::warn!("Received empty RustleGraph ID. Returning transparent image.");
                    return -1;
                }
                let (id, time) = id.rsplit_once(':').unwrap();
                let time: f64 = time.parse().unwrap();
                let slice = unsafe { std::slice::from_raw_parts_mut(buf, size_in_bytes) };

                let mut vizualizers = ctx.as_ref().expect("no null pointers").borrow_mut();
                if let Some(viz) = vizualizers.get(id) {
                    if let Some(viz) = viz.upgrade() {
                        let mut img = image::ImageBuffer::<image::Rgba<u8>, &mut [u8]>::from_raw(width, height, slice).expect("correct dimensions");
                        if let Err(e) = viz.render_to_image(rustlegraph::Time { seconds: time as u64, frac: time.fract()}, &mut img) {
                            tracing::error!("Could not render RustleGraph: {}", e);
                            return -2;
                        }
                    } else {
                        tracing::trace!("Viz was dropped at {}", id.to_string());
                        vizualizers.remove(id);
                        return -3;
                    }
                } else {
                    tracing::trace!("RustleGraph `{}' not found", id.to_string());
                    return -4;
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

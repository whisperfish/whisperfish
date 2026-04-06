//! # Whisperfish Model Patterns
//!
//! This module contains QML model implementations for the Whisperfish application.
//! For more details on the observation system, see:
//! - [`store::observer`](crate::store::observer) - Database observation framework
//! - [`observing_model` macro](active_model) - Model observation macros
//!
//! There are several patterns used for creating models:
//!
//! ## 1. Observing Model Pattern (with `observing_model` macro)
//!
//! Used for models that need to observe database changes and update automatically.
//! These models typically represent database entities and use the `observing_model` macro
//! from the `active_model` module.
//!
//! The `EventObserving` trait connects models to the database observation system,
//! allowing them to react to changes in the database tables they're interested in.
//!
//! **Note**: Most ORM models in Whisperfish follow this pattern, including:
//! - Message models (`messages.rs`)
//! - Session models (`sessions.rs`)
//! - Reaction models (`reactions.rs`)
//! - Group models (`group.rs`)
//! - Recipient models (`recipient.rs`)
//!
//! The `interests()` method is implemented for ORM entities in
//! [`store::observer::orm_interests`](crate::store::observer::orm_interests),
//! which provides the foundation for the observation system.
//!
//! Example: `messages.rs`, `sessions.rs`, `reactions.rs`
//!
//! ```ignore
//! use qmetaobject::prelude::*;
//!
//! #[observing_model]
//! #[derive(Default, QObject)]
//! pub struct MyModel {
//!     base: qt_base_class!(trait QObject),
//!     id: Option<i32>,
//!
//!     #[qt_property(
//!         READ: get_id,
//!         WRITE: set_id,
//!         NOTIFY: model_changed,
//!     )]
//!     myId: i32,
//!
//!     model_changed: qt_signal!(),
//! }
//!
//! impl EventObserving for MyModel {
//!     type Context = ModelContext<Self>;
//!
//!     fn observe(&mut self, ctx: Self::Context, event: Event) {
//!         // React to database events. See [`Event`](crate::store::observer::Event) for
//!         // available methods like `for_table()`, `for_row()`, `is_insert()`, etc.
//!         if event.for_table(schema::my_table::table) {
//!             self.fetch(ctx.storage());
//!             self.model_changed();
//!         }
//!     }
//!
//!     fn interests(&self) -> Vec<Interest> {
//!         // Declare which database changes this model is interested in.
//!         // See [`Interest`](crate::store::observer::Interest) for available methods.
//!         
//!         // The interests() method is crucial for the observation system - it tells
//!         // the framework which database changes should trigger observe() calls.
//!         // Most ORM models in Whisperfish implement this method to specify their
//!         // dependencies on specific tables, rows, or relationships.
//!         
//!         // For ORM entities, the default implementation is provided in
//!         // [`store::observer::orm_interests`](crate::store::observer::orm_interests).
//!         // Models can override this to add additional interests or modify behavior.
//!         if let Some(id) = self.id {
//!             vec![Interest::for_row(schema::my_table::table, id)]
//!         } else {
//!             Vec::new()
//!         }
//!     }
//! }
//! ```
//!
//! ## 2. Simple Property Model Pattern (with `qt_property!` macro)
//!
//! Used for models that don't need database observation but provide computed properties.
//! These models use the `qt_property!` macro for automatic property management.
//!
//! Unlike observing models, these models don't automatically react to database changes.
//! They're useful for computed properties or models that are recreated when needed.
//!
//! Example: `receipts.rs`
//!
//! The Receipts model demonstrates this pattern well - it provides three computed
//! properties (delivery_receipts, read_receipts, viewed_receipts) that format receipt
//! data for QML consumption, and automatically refreshes when the message_id changes.
//!
//! ```rust
//! use qmetaobject::prelude::*;
//! use whisperfish::gui::AppState;
//!
//! #[derive(Default, QObject)]
//! pub struct MyModel {
//!     base: qt_base_class!(trait QObject),
//!
//!     // Automatic property with getter/setter
//!     app: qt_property!(QPointer<AppState>; WRITE set_app),
//!     message_id: qt_property!(i32; WRITE set_message_id),
//!
//!     // Read-only computed property
//!     my_data: qt_property!(QVariant; READ compute_data),
//!
//!     data_changed: qt_signal!(),
//! }
//!
//! impl MyModel {
//!     // Setter methods (qt_property! macro handles storage)
//!     fn set_app(&mut self, _app: QPointer<AppState>) {
//!         self.data_changed();
//!     }
//!
//!     fn set_message_id(&mut self, _id: i32) {
//!         self.data_changed();
//!     }
//!
//!     // Getter method for computed property
//!     fn compute_data(&self) -> QVariant {
//!         // Compute and return data
//!         QVariant::default()
//!     }
//! }
//! ```
//!
//! ## 3. Model Roles Pattern (with `define_model_roles!` macro)
//!
//! Used for list models that expose database entities to QML.
//! This pattern maps Rust structs to QML-accessible properties.
//!
//! Example: `messages.rs` (MessageRoles), `sessions.rs` (SessionRoles)
//!
//! ```ignore
//! define_model_roles! {
//!     pub(super) enum MyModelRoles for orm::MyEntity {
//!         Id(id): "id",
//!         Name(name): "name",
//!         Timestamp(created_at via qdatetime_from_naive): "createdAt",
//!         Computed(fn compute_property(&self) via conversion_fn): "computed",
//!     }
//! }
//! ```
//!
//! ## Common Patterns
//!
//! ### Property Access
//! - Use `self.property_name` for direct field access (qt_property! macro)
//! - Use `self.property_name()` for getter method access (observing_model macro)
//!
//! ### Signals
//! - Always emit signals when data changes to notify QML
//! - Use `self.signal_name()` to emit signals
//!
//! ### Database Access
//! - Access storage through `ctx.storage()` in observing models
//! - Access storage through `app.borrow().storage.borrow().clone()` in simple models
//!
//! ### QML Data Formatting
//! - Use `QVariant`, `QVariantList`, `QVariantMap` for QML data
//! - Convert Rust types using `.to_qvariant()` (requires `QMetaType` trait)
//! - Use conversion functions like `QString::from()`, `qdatetime_from_naive()`, etc.
macro_rules! define_model_roles {
    (RETRIEVE $obj:ident fn $fn:ident(&self) $(via $via_fn:path)*) => {{
        let field = $obj.$fn();
        $(let field = $via_fn(field);)*
        field.into()
    }};
    (RETRIEVE $obj:ident $($field:ident).+ $(via $via_fn:path)*) => {{
        let field = $obj.$($field).+.clone();
        $(let field = $via_fn(field);)*
        field.into()
    }};
    ($vis:vis enum $enum_name:ident for $diesel_model:ty $([with offset $offset:literal])? {
     $($role:ident($($retrieval:tt)*): $name:expr),* $(,)?
    }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis enum $enum_name {
            $($role),*
        }

        impl $enum_name {
            #[allow(unused_assignments)]
            #[allow(dead_code)]
            $vis fn role_names() -> std::collections::HashMap<i32, qmetaobject::QByteArray> {
                let mut hm = std::collections::HashMap::new();

                let mut i = 0;
                $(i = $offset;)?
                $(
                    hm.insert(i, $name.into());
                    i += 1;
                )*

                hm
            }

            $vis fn get(&self, obj: &$diesel_model) -> qmetaobject::QVariant {
                match self {
                    $(
                        Self::$role => define_model_roles!(RETRIEVE obj $($retrieval)*),
                    )*
                }
            }

            #[allow(unused)]
            $vis fn from(i: i32) -> Self {
                let rm = [$(Self::$role, )*];
                rm[i as usize]
            }
        }
    };
}

mod active_model;
pub mod attachment;
#[cfg(feature = "calling")]
pub mod calling;
pub mod contact;
pub mod create_conversation;
pub mod device;
pub mod group;
pub mod grouped_reactions;
pub mod messages;
pub mod reactions;
pub mod receipts;
pub mod recipient;
pub mod rustlegraph;
pub mod sessions;
pub mod voice_note_recorder;

pub mod prompt;

use std::time::Duration;

pub use self::active_model::*;
pub use self::attachment::*;
#[cfg(feature = "calling")]
pub use self::calling::*;
pub use self::contact::*;
pub use self::create_conversation::*;
pub use self::device::*;
pub use self::group::*;
pub use self::grouped_reactions::*;
pub use self::messages::*;
pub use self::prompt::*;
pub use self::reactions::*;
pub use self::receipts::*;
pub use self::recipient::*;
pub use self::rustlegraph::*;
pub use self::sessions::*;
pub use self::voice_note_recorder::*;

use chrono::prelude::*;
use libsignal_protocol::DeviceId;
use qmetaobject::prelude::*;

fn qdate_from_chrono<T: TimeZone>(dt: DateTime<T>) -> QDate {
    let dt = dt.with_timezone(&Local).naive_local();
    QDate::from_y_m_d(dt.year(), dt.month() as i32, dt.day() as i32)
}

fn qstring_from_cow(cow: std::borrow::Cow<'_, str>) -> QString {
    QString::from(cow.as_ref())
}

fn qdatetime_from_chrono<T: TimeZone>(dt: DateTime<T>) -> QDateTime {
    let dt = dt.with_timezone(&Local).naive_local();
    let date = QDate::from_y_m_d(dt.year(), dt.month() as i32, dt.day() as i32);
    let time = QTime::from_h_m_s_ms(
        dt.hour() as i32,
        dt.minute() as i32,
        Some(dt.second() as i32),
        None,
    );

    QDateTime::from_date_time_local_timezone(date, time)
}

fn qdatetime_from_naive_option(timestamp: Option<NaiveDateTime>) -> qmetaobject::QVariant {
    timestamp
        .map(qdatetime_from_naive)
        .map(QVariant::from)
        .unwrap_or_default()
}

fn qdatetime_from_naive(timestamp: NaiveDateTime) -> QDateTime {
    // Naive in model is Utc, naive displayed should be Local
    qdatetime_from_chrono(timestamp.and_utc())
}

fn qstring_from_optional_to_string(opt: Option<impl ToString>) -> QVariant {
    match opt {
        Some(s) => QString::from(s.to_string()).into(),
        None => QVariant::default(),
    }
}

fn qstring_from_option(opt: Option<impl AsRef<str>>) -> QVariant {
    match opt {
        Some(s) => QString::from(s.as_ref()).into(),
        None => QVariant::default(),
    }
}

fn int_from_i32_option(val: Option<i32>) -> i32 {
    val.unwrap_or(-1)
}

fn int_from_duration_option(val: Option<Duration>) -> i32 {
    match val {
        Some(t) => t.as_secs() as _,
        None => -1,
    }
}

fn int_from_usize(val: usize) -> i32 {
    val as i32
}

fn int_from_device_id(val: DeviceId) -> u32 {
    val.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qdate_from_chrono() {
        std::env::set_var("TZ", "UTC");

        // Same day as at UTC
        let qdate = qdate_from_chrono::<FixedOffset>(
            DateTime::parse_from_rfc3339("1996-12-19T16:39:57+08:00").unwrap(),
        );
        assert_eq!(qdate.get_y_m_d(), (1996, 12, 19));

        // Different day as at UTC
        let qdate = qdate_from_chrono::<FixedOffset>(
            DateTime::parse_from_rfc3339("1996-12-19T16:39:57-08:00").unwrap(),
        );
        assert_eq!(qdate.get_y_m_d(), (1996, 12, 20));
    }

    #[test]
    fn test_qdatetime_from_chrono() {
        std::env::set_var("TZ", "UTC");

        // Same day as at UTC
        let qdatetime = qdatetime_from_chrono::<FixedOffset>(
            DateTime::parse_from_rfc3339("1996-12-19T16:39:57+08:00").unwrap(),
        );
        let (qdate, qtime) = qdatetime.get_date_time();
        assert_eq!(qdate.get_y_m_d(), (1996, 12, 19));
        assert_eq!(qtime.get_h_m_s_ms(), (8, 39, 57, 0));

        // Different day as at UTC
        let qdatetime = qdatetime_from_chrono::<FixedOffset>(
            DateTime::parse_from_rfc3339("1996-12-19T16:39:57-08:00").unwrap(),
        );
        let (qdate, qtime) = qdatetime.get_date_time();
        assert_eq!(qdate.get_y_m_d(), (1996, 12, 20));
        assert_eq!(qtime.get_h_m_s_ms(), (0, 39, 57, 0));
    }

    #[test]
    fn test_qdatetime_from_naive() {
        std::env::set_var("TZ", "UTC");

        let qdatetime = qdatetime_from_naive(
            chrono::NaiveDateTime::parse_from_str("1996-12-19 16:39:57", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
        );
        let (qdate, qtime) = qdatetime.get_date_time();
        assert_eq!(qdate.get_y_m_d(), (1996, 12, 19));
        assert_eq!(qtime.get_h_m_s_ms(), (16, 39, 57, 0));
    }

    #[test]
    fn test_qstring_from_option() {
        let s = qstring_from_option(Some("test"));
        assert_eq!(s.to_qstring().to_string(), String::from("test"));

        let s = qstring_from_option(None::<&str>);
        assert_eq!(s.to_qstring().to_string(), String::from(""));
    }
}

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
pub use self::recipient::*;
pub use self::rustlegraph::*;
pub use self::sessions::*;
pub use self::voice_note_recorder::*;

use chrono::prelude::*;
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

fn qvariant_from_option<T>(val: Option<T>) -> QVariant
where
    T: Into<QVariant>,
{
    match val {
        Some(s) => s.into(),
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

    #[test]
    fn test_qvariant_from_option() {
        let s = qvariant_from_option(Some(QString::from("test")));
        assert_eq!(s.to_qstring().to_string(), String::from("test"));

        let s = qvariant_from_option(None::<&QString>);
        assert_eq!(s.to_qstring().to_string(), String::from(""));
    }
}

macro_rules! define_model_roles {
    (RETRIEVE $obj:ident fn $fn:ident(&self) $(via $via_fn:path)*) => {{
        let field = $obj.$fn().clone();
        $(let field = $via_fn(field);)*
        field.into()
    }};
    (RETRIEVE $obj:ident $field:ident $(via $via_fn:path)*) => {{
        let field = $obj.$field.clone();
        $(let field = $via_fn(field);)*
        field.into()
    }};
    (enum $enum_name:ident for $diesel_model:ty {
     $($role:ident($($retrieval:tt)*): $name:expr),* $(,)?
    }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum $enum_name {
            $($role),*
        }

        impl $enum_name {
            #[allow(unused_assignments)]
            fn role_names() -> HashMap<i32, QByteArray> {
                let mut hm = HashMap::new();

                let mut i = 0;
                $(
                    hm.insert(i, $name.into());
                    i += 1;
                )*

                hm
            }

            fn get(&self, obj: &$diesel_model) -> QVariant {
                match self {
                    $(
                        Self::$role => define_model_roles!(RETRIEVE obj $($retrieval)*),
                    )*
                }
            }

            fn from(i: i32) -> Self {
                let rm = [$(Self::$role, )*];
                rm[i as usize]
            }
        }
    };
}

pub mod contact;
pub mod device;
pub mod message;
pub mod session;

pub mod filepicker;
pub mod prompt;

pub use contact::*;
pub use device::*;
pub use message::*;
pub use session::*;

pub use filepicker::*;
pub use prompt::*;

use chrono::prelude::*;
use qmetaobject::*;

fn qdatetime_from_i64(timestamp: i64) -> QDateTime {
    let dt = NaiveDateTime::from_timestamp(timestamp / 1000, (timestamp % 1000) as u32);
    let date = QDate::from_y_m_d(dt.year(), dt.month() as i32, dt.day() as i32);
    let time = QTime::from_h_m_s_ms(dt.hour() as i32, dt.minute() as i32, Some(dt.second() as i32), None);

    QDateTime::from_date_time_local_timezone(date, time)
}

fn qstring_from_option(opt: Option<String>) -> QVariant {
    match opt {
        Some(s) => QString::from(s).into(),
        None => QVariant::default(),
    }
}

use std::collections::HashMap;
use std::str::FromStr;

use crate::settings::*;

use diesel::prelude::*;
use phonenumber::Mode;
use qmetaobject::*;

/// `/home/nemo/.local/share/system/Contacts/qtcontacts-sqlite/contacts.db`
///
/// Contains only the part after `share`.
const DB_PATH: &str = "system/Contacts/qtcontacts-sqlite/contacts.db";

#[derive(QObject, Default)]
pub struct ContactModel {
    base: qt_base_class!(trait QAbstractListModel),
    // actor: Option<Addr<ContactActor>>,
    content: Vec<Contact>,

    refresh: qt_method!(fn(&mut self)),
    format: qt_method!(fn(&self, string: QString) -> QString),
    name: qt_method!(fn(&self, source: QString) -> QString),

    total: qt_property!(i32; NOTIFY contacts_changed READ total),
    count: qt_property!(i32; NOTIFY contacts_changed READ row_count),

    contacts_changed: qt_signal!(),
}

#[derive(Queryable, Clone, Debug)]
pub struct Contact {
    name: String,
    tel: String,
}

define_model_roles! {
    enum ContactRoles for Contact {
        Name(name via QString::from): "name",
        Tel(tel via QString::from):   "tel"
    }
}

impl ContactModel {
    // The default formatter expected by QML
    fn format(&self, number: QString) -> QString {
        let settings = Settings::default();
        let country_code = settings.get_string("country_code");

        format_with_country(&number.to_string(), &country_code)
            .unwrap_or_else(|| "".into())
            .into()
    }

    fn db(&self) -> SqliteConnection {
        let path = dirs::data_local_dir().expect("find data directory");
        SqliteConnection::establish(path.join(DB_PATH).to_str().expect("UTF-8 path"))
            .expect("open contact database")
    }

    fn name(&self, source: QString) -> QString {
        use crate::contacts_schema::contacts;
        use crate::contacts_schema::phoneNumbers;

        let settings = Settings::default();
        let country_code = settings.get_string("country_code");

        let source = source.to_string();
        let source = source.trim();

        let conn = self.db(); // This should maybe be established only once

        // This will ensure the format to query is ok
        let e164_source = format_with_country_helper(&source, Mode::E164, &country_code)
            .unwrap_or_else(|| "".into());
        let mut national_source =
            format_with_country_helper(&source, Mode::National, &country_code)
                .unwrap_or_else(|| "".into());
        national_source.retain(|c| c != ' '); // At least FI numbers had spaces after parsing
        let doppio_zero = e164_source.clone().replace('+', "00");
        let source = source.to_string();

        let trimmed_number = diesel::dsl::sql::<diesel::sql_types::Text>(
            "REPLACE(phoneNumbers.phoneNumber, ' ', '')",
        );
        let trimmed_number = &trimmed_number;

        let (name, _phone_number): (String, String) = contacts::table
            .inner_join(phoneNumbers::table)
            .select((contacts::displayLabel, trimmed_number))
            .filter(trimmed_number.like(&e164_source))
            .or_filter(trimmed_number.like(&national_source))
            .or_filter(trimmed_number.like(&doppio_zero))
            .get_result(&conn)
            .unwrap_or((source.clone(), source));

        QString::from(name)
    }

    pub fn refresh(&mut self) {
        log::info!("Refreshing contacts");
        use crate::contacts_schema::contacts;
        use crate::contacts_schema::phoneNumbers;

        let settings = crate::settings::Settings::default();
        let country_code = settings.get_string("country_code");
        let db = self.db();

        let country = match phonenumber::country::Id::from_str(&country_code) {
            Ok(country) => Some(country),
            Err(()) => {
                log::warn!("Please set country in settings!");
                None
            }
        };

        let contacts: Result<Vec<Contact>, _> = contacts::table
            .inner_join(phoneNumbers::table)
            .select((contacts::displayLabel, phoneNumbers::phoneNumber))
            .order_by(contacts::displayLabel.asc())
            .get_results(&db);

        (self as &mut dyn QAbstractListModel).begin_reset_model();

        match contacts {
            Ok(contacts) => {
                log::info!("Found {} contacts", contacts.len());
                self.content = contacts;
            }
            Err(e) => {
                log::error!("Refreshing contacts {}", e);
                return;
            }
        }

        for contact in self.content.iter_mut() {
            let number = match phonenumber::parse(country, &contact.tel) {
                Ok(number) => number,
                Err(e) => {
                    log::warn!("Could not format phone number: {}", e);
                    continue;
                }
            };
            contact.tel = number.format().mode(phonenumber::Mode::E164).to_string();
        }
        (self as &mut dyn QAbstractListModel).end_reset_model();

        self.contacts_changed();
    }

    fn total(&self) -> i32 {
        // XXX: this should in fact be the amount of *registered* contacts.
        self.row_count()
    }
}

fn format_with_country_helper(number: &str, mode: Mode, country_code: &str) -> Option<String> {
    let country = phonenumber::country::Id::from_str(country_code).ok();

    log::trace!("Formatting phone number with country {:?}", country);

    let number = match phonenumber::parse(country, number) {
        Ok(number) => number,
        Err(_) => return None,
    };

    log::trace!("Formatted phone number: {:?}", number);

    if !phonenumber::is_valid(&number) {
        log::warn!(
            "Phone number is invalid according to the `phonenumber` library. Proceed with caution"
        );
        // return None;
    }

    Some(number.format().mode(mode).to_string())
}

fn format_with_country(number: &str, country_code: &str) -> Option<String> {
    let number = number.trim();
    if number.is_empty() {
        return None;
    }

    let try_with_plus = if !number.starts_with('+') {
        let number_with_plus = format!("+{}", number);
        format_with_country_helper(&number_with_plus, Mode::E164, &country_code)
    } else {
        None
    };

    format_with_country_helper(number, Mode::E164, &country_code).or(try_with_plus)
}

impl QAbstractListModel for ContactModel {
    fn row_count(&self) -> i32 {
        log::trace!("ContactModel::row_count");
        self.content.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        log::trace!("ContactModel::data(role={})", role);
        let role = ContactRoles::from(role);
        role.get(&self.content[index.row() as usize])
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        log::trace!("ContactModel::role_names");
        ContactRoles::role_names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    // 00-prefixed numbers tracking issue:
    // https://github.com/rustonaut/rust-phonenumber/issues/29
    #[rstest(
        phone,
        case("+32474123456"),
        // case("0032474123456"),
        case("+3541234567"),
        // case("003541234567"),
        case("+18875550100"),
        // case("0018875550100")
    )]
    fn e164_phone_number_acceptance_test(phone: &str) {
        env_logger::try_init().ok();
        assert!(
            format_with_country(phone, "").is_some(),
            "phone '{}' is not accepted without country",
            phone
        )
    }

    #[rstest(
        phone,
        country,
        case("0474123456", "BE"),
        case("01234567", "IS"),
        case("08875550100", "US")
    )]
    fn local_phone_number_acceptance_test(phone: &str, country: &str) {
        env_logger::try_init().ok();
        assert!(
            format_with_country(phone, country).is_some(),
            "phone '{}' with country '{}' is not accepted",
            phone,
            country
        )
    }
}

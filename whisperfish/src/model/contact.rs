use crate::config::SettingsBridge;
use phonenumber::Mode;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use std::str::FromStr;

#[derive(QObject, Default)]
pub struct ContactModel {
    base: qt_base_class!(trait QObject),

    format: qt_method!(fn(&self, string: QString) -> QString),

    total: qt_property!(i32; NOTIFY contacts_changed READ total),

    contacts_changed: qt_signal!(),
}

impl ContactModel {
    // The default formatter expected by QML
    #[with_executor]
    fn format(&self, number: QString) -> QString {
        let settings = SettingsBridge::default();
        let country_code = settings.get_string("country_code");

        format_with_country(&number.to_string(), &country_code)
            .unwrap_or_else(|| "".into())
            .into()
    }

    fn total(&self) -> i32 {
        // XXX: this should in fact be the amount of *registered* contacts.
        0
    }
}

fn format_with_country_helper(number: &str, mode: Mode, country_code: &str) -> Option<String> {
    let country = phonenumber::country::Id::from_str(country_code).ok();

    let number = match phonenumber::parse(country, number) {
        Ok(number) => number,
        Err(_) => return None,
    };

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
        format_with_country_helper(&number_with_plus, Mode::E164, country_code)
    } else {
        None
    };

    format_with_country_helper(number, Mode::E164, country_code).or(try_with_plus)
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
        assert!(
            format_with_country(phone, country).is_some(),
            "phone '{}' with country '{}' is not accepted",
            phone,
            country
        )
    }
}

use std::{convert::TryFrom, fmt::Display};

use anyhow::Context;
use diesel::{
    backend, deserialize, serialize,
    sql_types::{Integer, Nullable, Text},
    Expression,
};
use phonenumber::PhoneNumber;
use uuid::Uuid;

use super::{DistributionListPrivacyMode, StoryType, UnidentifiedAccessMode};

impl<DB> deserialize::FromSql<Integer, DB> for StoryType
where
    DB: backend::Backend,
    i32: deserialize::FromSql<Integer, DB>,
{
    fn from_sql(bytes: <DB>::RawValue<'_>) -> deserialize::Result<Self> {
        let i = i32::from_sql(bytes)?;
        match StoryType::try_from(i) {
            Ok(x) => Ok(x),
            Err(_) => Err(format!("Unrecognized StoryType variant {}", i).into()),
        }
    }
}

impl<DB> deserialize::FromSql<Integer, DB> for UnidentifiedAccessMode
where
    DB: backend::Backend,
    i32: deserialize::FromSql<Integer, DB>,
{
    fn from_sql(bytes: <DB>::RawValue<'_>) -> deserialize::Result<Self> {
        let i = i32::from_sql(bytes)?;
        match UnidentifiedAccessMode::try_from(i) {
            Ok(x) => Ok(x),
            Err(_) => Err(format!("Unrecognized UnidentifiedAccessMode variant {}", i).into()),
        }
    }
}

// Diesel really doesn't like having an Err variant, and apparently we unwrap the errors without
// Rust being able to print a backtrace.  This makes for very undebuggable errors, see e.g. https://gitlab.com/whisperfish/whisperfish/-/merge_requests/462
// For that reason, we deserialize invalid values to None instead, and log the error.
//
// Semantically this is usually correct: the invalid field should be replaced at a certain point.
fn log_error_return_none<T>(res: anyhow::Result<Option<T>>) -> Option<T> {
    match res {
        Err(e) => {
            tracing::error!(
                "Error deserializing: {}. Please file an issue if this error persists.",
                e
            );
            None
        }
        Ok(x) => x,
    }
}

fn log_error<T>(res: anyhow::Result<T>) -> anyhow::Result<T> {
    if let Err(e) = &res {
        tracing::error!("Error deserializing; this will crash: {}", e);
    }
    res
}

impl serialize::ToSql<Integer, diesel::sqlite::Sqlite> for DistributionListPrivacyMode
where
    i32: serialize::ToSql<Integer, diesel::sqlite::Sqlite>,
{
    fn to_sql<'b>(
        &'b self,
        out: &mut serialize::Output<'b, '_, diesel::sqlite::Sqlite>,
    ) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(serialize::IsNull::No)
    }
}

impl serialize::ToSql<Integer, diesel::sqlite::Sqlite> for StoryType
where
    i32: serialize::ToSql<Integer, diesel::sqlite::Sqlite>,
{
    fn to_sql<'b>(
        &'b self,
        out: &mut serialize::Output<'b, '_, diesel::sqlite::Sqlite>,
    ) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(serialize::IsNull::No)
    }
}

impl serialize::ToSql<Integer, diesel::sqlite::Sqlite> for UnidentifiedAccessMode
where
    i32: serialize::ToSql<Integer, diesel::sqlite::Sqlite>,
{
    fn to_sql<'b>(
        &'b self,
        out: &mut serialize::Output<'b, '_, diesel::sqlite::Sqlite>,
    ) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(serialize::IsNull::No)
    }
}

#[derive(Debug)]
pub struct OptionUuidString(Option<Uuid>);
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct UuidString(Uuid);

impl Display for UuidString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Expression for UuidString {
    type SqlType = Text;
}

impl<DB> deserialize::Queryable<Nullable<Text>, DB> for OptionUuidString
where
    DB: backend::Backend,
    Option<String>: deserialize::FromSql<Nullable<Text>, DB>,
{
    type Row = Option<String>;

    fn build(s: Option<String>) -> diesel::deserialize::Result<Self> {
        let uuid = s
            .as_deref()
            .filter(|x| !x.is_empty())
            .map(Uuid::parse_str)
            .transpose()
            .with_context(|| format!("OptionUuidString: deserializing {:?}", s));
        Ok(OptionUuidString(log_error_return_none(uuid)))
    }
}

impl serialize::ToSql<Text, diesel::sqlite::Sqlite> for UuidString
where
    String: serialize::ToSql<Text, diesel::sqlite::Sqlite>,
{
    fn to_sql<'b>(
        &'b self,
        out: &mut serialize::Output<'b, '_, diesel::sqlite::Sqlite>,
    ) -> serialize::Result {
        out.set_value(self.to_string());
        Ok(serialize::IsNull::No)
    }
}

impl<DB> deserialize::Queryable<Text, DB> for UuidString
where
    DB: backend::Backend,
    String: deserialize::FromSql<Text, DB>,
{
    type Row = String;

    fn build(s: String) -> diesel::deserialize::Result<Self> {
        let uuid =
            Uuid::parse_str(&s).with_context(|| format!("UuidString: deserializing {:?}", s));
        Ok(UuidString(log_error(uuid)?))
    }
}

impl From<UuidString> for Uuid {
    fn from(val: UuidString) -> Self {
        val.0
    }
}

impl From<uuid::Uuid> for UuidString {
    fn from(value: uuid::Uuid) -> Self {
        UuidString(value)
    }
}

impl From<OptionUuidString> for Option<Uuid> {
    fn from(val: OptionUuidString) -> Self {
        val.0
    }
}

pub struct OptionPhoneNumberString(Option<PhoneNumber>);
pub struct PhoneNumberString(PhoneNumber);

impl<DB> deserialize::Queryable<Nullable<Text>, DB> for OptionPhoneNumberString
where
    DB: backend::Backend,
    Option<String>: deserialize::FromSql<Nullable<Text>, DB>,
{
    type Row = Option<String>;

    fn build(s: Option<String>) -> diesel::deserialize::Result<Self> {
        let phonenumber = s
            .as_deref()
            // XXX: a migration should be made to set these to NULL instead in the db.
            .filter(|x| !x.is_empty())
            .map(|s| phonenumber::parse(None, s))
            .transpose()
            .with_context(|| format!("OptionPhoneNumberString: deserializing {:?}", s));
        Ok(OptionPhoneNumberString(log_error_return_none(phonenumber)))
    }
}

impl<DB> deserialize::Queryable<Text, DB> for PhoneNumberString
where
    DB: backend::Backend,
    String: deserialize::FromSql<Text, DB>,
{
    type Row = String;

    fn build(s: String) -> diesel::deserialize::Result<Self> {
        let phonenumber = phonenumber::parse(None, &s)
            .with_context(|| format!("PhoneNumberString: deserializing {}", s));
        Ok(PhoneNumberString(log_error(phonenumber)?))
    }
}

impl From<PhoneNumberString> for PhoneNumber {
    fn from(val: PhoneNumberString) -> Self {
        val.0
    }
}

impl From<OptionPhoneNumberString> for Option<PhoneNumber> {
    fn from(val: OptionPhoneNumberString) -> Self {
        val.0
    }
}

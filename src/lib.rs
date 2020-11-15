#![recursion_limit = "512"]
#![deny(rust_2018_idioms)]

#[macro_use]
extern crate cpp;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

pub mod sfos;

pub mod actor;
pub mod model;

pub mod worker;

pub mod contacts_schema;
pub mod schema;

pub mod settings;

pub mod gui;
pub mod store;

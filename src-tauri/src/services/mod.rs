//! Application services orchestrating `mangacore`, SQLite repositories, caching, and notifications.

pub(crate) mod ai;
pub(crate) mod app_release;
pub(crate) mod backup_service;
pub(crate) mod comics;
pub(crate) mod layout_detect;
pub(crate) mod library;
pub(crate) mod reader;
pub(crate) mod rtmaga_codec;
pub(crate) mod sources;
pub(crate) mod sync;
pub(crate) mod updater;

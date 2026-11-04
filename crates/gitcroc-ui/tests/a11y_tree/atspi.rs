//! What a screen reader hears over AT-SPI, the port of M6-04's
//! `atspi-probe.py`: the session's bus switched on so that GPUI's AccessKit
//! adapter activates, the announcements live nodes make, and the states
//! of the window's dialogs. The tree dump has neither live nor modal, so
//! these are read here, as Orca reads them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use atspi_common::{Role, State, StateSet};
use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
/// Every application's root, and the registry's.
const ROOT: &str = "/org/a11y/atspi/accessible/root";
/// How deep the walk for dialogs goes: the window's tree is far shallower.
const MAX_DEPTH: usize = 40;

/// Tells the session that assistive technology is running, as Orca's
/// start does, so the window builds its tree.
pub fn enable() -> Result<(), String> {
    let session = Connection::session().map_err(|error| format!("no D-Bus session: {error}"))?;
    session
        .call_method(
            Some("org.a11y.Bus"),
            "/org/a11y/bus",
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &("org.a11y.Status", "IsEnabled", Value::from(true)),
        )
        .map_err(|error| format!("cannot enable the AT-SPI bus: {error}"))?;
    Ok(())
}

/// A connection to the AT-SPI bus, which is not the session bus.
pub fn connect() -> Result<Connection, String> {
    let session = Connection::session().map_err(|error| format!("no D-Bus session: {error}"))?;
    let address: String = session
        .call_method(
            Some("org.a11y.Bus"),
            "/org/a11y/bus",
            Some("org.a11y.Bus"),
            "GetAddress",
            &(),
        )
        .and_then(|reply| reply.body().deserialize())
        .map_err(|error| format!("no AT-SPI bus address: {error}"))?;
    zbus::blocking::connection::Builder::address(address.as_str())
        .and_then(|builder| builder.build())
        .map_err(|error| format!("cannot connect to the AT-SPI bus: {error}"))
}

/// Every announcement since [`Announcements::listen`], as a screen reader
/// receives them: the text and its politeness (1 polite, 2 assertive).
#[derive(Clone, Default)]
pub struct Announcements(Arc<Mutex<Vec<(String, i32)>>>);

impl Announcements {
    /// Listens on a thread of its own for as long as the process runs.
    pub fn listen() -> Result<Self, String> {
        let connection = connect()?;
        let rule = "type='signal',interface='org.a11y.atspi.Event.Object',member='Announcement'";
        let rule =
            zbus::MatchRule::try_from(rule).map_err(|error| format!("bad match rule: {error}"))?;
        let messages = MessageIterator::for_match_rule(rule, &connection, None)
            .map_err(|error| format!("cannot listen for announcements: {error}"))?;
        let heard = Self::default();
        let store = heard.0.clone();
        std::thread::spawn(move || {
            // The connection must outlive the iterator's subscription.
            let _connection = connection;
            for message in messages.flatten() {
                type Body = (String, i32, i32, OwnedValue, HashMap<String, OwnedValue>);
                if let Ok((_, politeness, _, text, _)) = message.body().deserialize::<Body>() {
                    let text = String::try_from(text).unwrap_or_default();
                    store.lock().unwrap().push((text, politeness));
                }
            }
        });
        Ok(heard)
    }

    /// What was announced so far, oldest first.
    pub fn all(&self) -> Vec<(String, i32)> {
        self.0.lock().unwrap().clone()
    }
}

/// A dialog as AT-SPI exposes it.
#[derive(Debug)]
pub struct Dialog {
    pub name: String,
    pub modal: bool,
}

/// Every dialog or alert dialog of this process's windows.
pub fn dialogs(connection: &Connection) -> Result<Vec<Dialog>, String> {
    let mut found = Vec::new();
    for (app, path) in children(connection, "org.a11y.atspi.Registry", ROOT)? {
        if pid(connection, &app) == Some(std::process::id()) {
            walk(connection, &app, &path, 0, &mut found)?;
        }
    }
    Ok(found)
}

fn walk(
    connection: &Connection,
    app: &str,
    path: &str,
    depth: usize,
    found: &mut Vec<Dialog>,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let role: u32 = call(connection, app, path, "GetRole")?;
    if matches!(Role::try_from(role), Ok(Role::Dialog | Role::Alert)) {
        let states: Vec<u32> = call(connection, app, path, "GetState")?;
        let bits = states
            .iter()
            .enumerate()
            .take(2)
            .fold(0u64, |bits, (i, word)| {
                bits | (u64::from(*word) << (32 * i))
            });
        let modal = StateSet::from_bits(bits).is_ok_and(|set| set.contains(State::Modal));
        found.push(Dialog {
            name: name(connection, app, path),
            modal,
        });
    }
    for (child_app, child) in children(connection, app, path)? {
        walk(connection, &child_app, &child, depth + 1, found)?;
    }
    Ok(())
}

fn children(
    connection: &Connection,
    app: &str,
    path: &str,
) -> Result<Vec<(String, String)>, String> {
    let children: Vec<(String, OwnedObjectPath)> = call(connection, app, path, "GetChildren")?;
    Ok(children
        .into_iter()
        .map(|(app, path)| (app, path.to_string()))
        .collect())
}

fn call<T>(connection: &Connection, app: &str, path: &str, method: &str) -> Result<T, String>
where
    T: for<'de> serde::Deserialize<'de> + zbus::zvariant::Type,
{
    connection
        .call_method(Some(app), path, Some(ACCESSIBLE), method, &())
        .and_then(|reply| reply.body().deserialize())
        .map_err(|error| format!("{method} of {app} {path}: {error}"))
}

fn name(connection: &Connection, app: &str, path: &str) -> String {
    connection
        .call_method(
            Some(app),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(ACCESSIBLE, "Name"),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default()
}

/// The process behind bus name `app`.
fn pid(connection: &Connection, app: &str) -> Option<u32> {
    connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "GetConnectionUnixProcessID",
            &(app,),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize().ok())
}

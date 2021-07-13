use crate::common::{ClipboardProvider, Result};
use crate::ContentType;
use std::convert::TryInto;
use std::time::{SystemTime, UNIX_EPOCH};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, ConnectionExt, CreateWindowAux, EventMask, GetPropertyReply, Gravity, Timestamp, Window,
    WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub struct X11RbClipboardContext {
    connection: RustConnection,
    window: Window,

    clipboard: Atom,
    utf8_string: Atom,
    targets: Atom,
    property: Atom,
    atom: Atom,
}

impl X11RbClipboardContext {
    pub fn new() -> Result<Self> {
        let (connection, screen_num) = RustConnection::connect(None)?;
        let screen = &connection.setup().roots[screen_num];
        let window = connection.generate_id()?;

        let win_aux = CreateWindowAux::new();
        let cookie = connection.create_window(
            screen.root_depth,
            window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &win_aux,
        )?;
        cookie.check()?;

        let clipboard = intern_atom(&connection, "CLIPBOARD")?;
        let utf8_string = intern_atom(&connection, "UTF8_STRING")?;
        let targets = intern_atom(&connection, "TARGETS")?;
        let property = intern_atom(&connection, "PROPERTY")?;
        let atom = intern_atom(&connection, "ATOM")?;
        Ok(Self { connection, window, clipboard, utf8_string, targets, property, atom })
    }

    fn get_full_property<A, B>(
        &self,
        delete: bool,
        window: Window,
        property: A,
        type_: B,
    ) -> Result<GetPropertyReply>
    where
        A: Into<Atom>,
        B: Into<Atom>,
    {
        let cookie = self.connection.get_property(delete, window, property, type_, 0, u32::MAX)?;
        let reply = cookie.reply()?;
        debug_assert_eq!(reply.bytes_after, 0);
        Ok(reply)
    }
}

impl ClipboardProvider for X11RbClipboardContext {
    fn get_contents(&self) -> Result<String> {
        let cookie = self.connection.convert_selection(
            self.window,
            self.clipboard,
            self.utf8_string,
            self.property,
            current_time(),
        )?;
        cookie.check()?;
        self.connection.flush()?;

        loop {
            let event = self.connection.wait_for_event()?;
            match event {
                Event::SelectionNotify(_ev) => {
                    let val = self.get_full_property(
                        false,
                        self.window,
                        self.property,
                        self.utf8_string,
                    )?;
                    return String::from_utf8(val.value).map_err(|e| Box::new(e) as _);
                },
                _ => {
                    dbg!("Have event {:?}", event);
                },
            }
        }
    }

    fn set_contents(&self, _: String) -> Result<()> {
        todo!()
    }

    fn get_content_types(&self) -> Result<Vec<ContentType>> {
        let cookie = self.connection.convert_selection(
            self.window,
            self.clipboard,
            self.targets,
            self.property,
            current_time(),
        )?;
        cookie.check()?;
        self.connection.flush()?;

        loop {
            let event = self.connection.wait_for_event()?;
            match event {
                Event::SelectionNotify(_ev) => {
                    let val =
                        self.get_full_property(false, self.window, self.property, self.atom)?;
                    let mut cts = Vec::new();
                    for atom in val.value32().ok_or("invalid response format for targets")? {
                        // TODO convert atom names correctly here
                        cts.push(ContentType::Custom(atom_name(&self.connection, atom)?))
                    }
                    return Ok(cts);
                },
                _ => {
                    dbg!("Have event {:?}", event);
                },
            }
        }
    }
}

fn intern_atom(connection: &RustConnection, name: &str) -> Result<Atom> {
    Ok(connection.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

fn atom_name(connection: &RustConnection, atom: Atom) -> Result<String> {
    let reply = connection.get_atom_name(atom)?.reply()?;
    String::from_utf8(reply.name).map_err(|e| Box::new(e) as _)
}

fn current_time() -> Timestamp {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
    since_the_epoch.as_secs().try_into().expect("if you're using this past 2k38, hmmmmmmmm")
}

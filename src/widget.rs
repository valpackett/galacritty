use std::ptr;
use std::rc::Rc;
use std::sync::Arc;
use std::cell::RefCell;
// use std::thread::JoinHandle;

use epoxy;
use shared_library::dynamic_library::DynamicLibrary;

use glib;
use gdk;
use gdk::ModifierType as Mod;
use gtk;
use gtk::prelude::*;

use alacritty::{cli, gl};
use alacritty::display::{Display, DisplayCommand, InitialSize};
use alacritty::event_loop::{self, EventLoop, WindowNotifier};
use alacritty::tty::{self, Pty};
use alacritty::sync::FairMutex;
use alacritty::term::{Term, SizeInfo};
use alacritty::config::Config;

// TODO vec for multiple widgets
thread_local!{
    static GLOBAL: RefCell<Option<gtk::GLArea>> = RefCell::new(None);
}

pub struct IsControlHeld(bool);

pub enum Event {
    Blank,
    CharInput(char, IsControlHeld),
    StringInput(String),
    StrInput(&'static str),
    WindowResized(u32, u32),
    HiDPIFactorChanged(f32),
    ChangeFontSize(i8),
    ResetFontSize,
}

struct Notifier;

impl WindowNotifier for Notifier {
    fn notify(&self) {
        // NOTE: not gtk::idle_add, that one checks if we're on the main thread
        let _ = glib::idle_add(|| {
            GLOBAL.with(|global| {
                if let Some(ref glarea) = *global.borrow() {
                    glarea.queue_draw();
                }
            });
            glib::Continue(false)
        });
    }
}

pub struct State {
    config: Config,
    display: Display,
    terminal: Arc<FairMutex<Term>>,
    pty: Pty,
    loop_notifier: event_loop::Notifier,
    pub event_queue: Vec<Event>,
}

/// Creates a GLArea that runs an Alacritty terminal emulator.
///
/// Eventually should be a GObject subclass, usable outside of Rust.
pub fn alacritty_widget(header_bar: gtk::HeaderBar) -> (gtk::GLArea, Rc<RefCell<Option<State>>>) {
    let glarea = gtk::GLArea::new();

    let im = gtk::IMMulticontext::new();
    im.set_use_preedit(false);

    let state: Rc<RefCell<Option<State>>> = Rc::new(RefCell::new(None));

    glarea.connect_realize(clone!(state, im => move |glarea| {
        let mut state = state.borrow_mut();
        im.set_client_window(glarea.get_window().as_ref());
        glarea.make_current();

        epoxy::load_with(|s| {
            unsafe {
                match DynamicLibrary::open(None).unwrap().symbol(s) {
                    Ok(v) => v,
                    Err(_) => ptr::null(),
                }
            }
        });
        gl::load_with(epoxy::get_proc_addr);

        let config = Config::default();
        let mut options = cli::Options::default();
        options.print_events = true;

        let display = Display::new(
            &config,
            InitialSize::Cells(config.dimensions()),
            glarea.get_scale_factor() as f32
        ).expect("Display::new");

        let terminal = Term::new(&config, display.size().to_owned());
        let terminal = Arc::new(FairMutex::new(terminal));

        let pty = tty::new(&config, &options, &display.size(), None);

        let event_loop = EventLoop::new(
            Arc::clone(&terminal),
            Box::new(Notifier),
            pty.reader(),
            options.ref_test,
        );

        let loop_notifier = event_loop::Notifier(event_loop.channel());
        let _io_thread = event_loop.spawn(None);

        *state = Some(State {
            config, display, terminal, pty,
            loop_notifier,
            event_queue: Vec::new()
        });
    }));

    glarea.connect_unrealize(clone!(state => move |_widget| {
        let mut state = state.borrow_mut();
        *state = None;
    }));

    glarea.connect_render(clone!(state, im => move |_glarea, _glctx| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            let mut terminal = state.terminal.lock();
            for event in state.event_queue.drain(..) {
                match event {
                    Event::Blank => (),
                    Event::CharInput(c, IsControlHeld(is_ctrl)) => {
                        let len = c.len_utf8();
                        let mut bytes = Vec::with_capacity(len);
                        unsafe {
                            bytes.set_len(len);
                            c.encode_utf8(&mut bytes[..]);
                        }
                        if is_ctrl {
                            for ch in bytes.iter_mut() {
                                if *ch >= 0x40 && *ch < 0x80 {
                                    *ch = *ch & !0x60;
                                }
                            }
                        }
                        use alacritty::event::Notify;
                        state.loop_notifier.notify(bytes);
                    },
                    Event::StrInput(s) => {
                        use alacritty::event::Notify;
                        state.loop_notifier.notify(s.as_bytes().to_vec());
                    },
                    Event::StringInput(s) => {
                        use alacritty::event::Notify;
                        state.loop_notifier.notify(s.as_bytes().to_vec());
                    },
                    Event::WindowResized(w, h) => {
                        state.display.command_channel().send(DisplayCommand::NewSize(w, h)).expect("send new size");
                        terminal.dirty = true;
                    },
                    Event::HiDPIFactorChanged(dpr) => {
                        // state.display.update_glyph_cache(&state.config, Some(fac))
                        // ^^^ bad somehow? Is the channel really necessary
                        state.display.command_channel().send(DisplayCommand::NewHiDPIFactor(dpr)).expect("send new dpr");
                        terminal.dirty = true;
                    },
                    Event::ChangeFontSize(delta) => {
                        terminal.change_font_size(delta);
                    },
                    Event::ResetFontSize => {
                        terminal.reset_font_size();
                    }
                }
            }
            if let Some(title) = terminal.get_next_title() {
                header_bar.set_title(&*title);
            }
            if terminal.needs_draw() {
                let (x, y) = state.display.current_xim_spot(&terminal);
                let &SizeInfo { cell_width, cell_height, .. } = state.display.size();
                im.set_cursor_location(&gtk::Rectangle {
                    x: x.into(), y: y.into(), width: cell_width as i32, height: cell_height as i32
                });
                state.display.handle_resize(&mut terminal, &state.config, &mut [&mut state.pty]);
                state.display.draw(terminal, &state.config, None, true);
            }
        }
        Inhibit(false)
    }));

    glarea.connect_resize(clone!(state => move |glarea, w, h| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(Event::WindowResized(w as u32, h as u32));
        }
        glarea.queue_draw();
    }));

    glarea.add_events(gdk::EventMask::KEY_PRESS_MASK.bits() as i32);

    glarea.connect_key_press_event(clone!(state, im => move |glarea, event| {
        if im.filter_keypress(event) {
            return Inhibit(true);
        }
        let kv = event.get_keyval();
        trace!("non-IM input: keyval {:?} unicode {:?}", kv, gdk::keyval_to_unicode(kv));
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            use gdk::enums::key::*;
            let mods = event.get_state();
            // TODO: make this dynamically configurable
            #[allow(non_upper_case_globals)] // they're not mine, why complain here?!
            state.event_queue.push(match kv {
                Page_Up | KP_Page_Up if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[5;2~"),
                Page_Up | KP_Page_Up if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[5;5~"),
                Page_Up | KP_Page_Up => Event::StrInput("\x1b[5~"),
                Page_Down | KP_Page_Down if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[6;2~"),
                Page_Down | KP_Page_Down if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[6;5~"),
                Page_Down | KP_Page_Down => Event::StrInput("\x1b[6~"),
                Tab if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[Z"),
                Insert => Event::StrInput("\x1b[2~"),
                Delete | KP_Delete => Event::StrInput("\x1b[3~"),
                Left | KP_Left if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2D"),
                Left | KP_Left if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;3D"),
                Left | KP_Left if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5D"),
                Left | KP_Left => Event::StrInput("\x1b[D"),
                Right | KP_Right if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2C"),
                Right | KP_Right if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;3C"),
                Right | KP_Right if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5C"),
                Right | KP_Right => Event::StrInput("\x1b[C"),
                Up | KP_Up if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2A"),
                Up | KP_Up if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;3A"),
                Up | KP_Up if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5A"),
                Up | KP_Up => Event::StrInput("\x1b[A"),
                Down | KP_Down if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2B"),
                Down | KP_Down if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;3B"),
                Down | KP_Down if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5B"),
                Down | KP_Down => Event::StrInput("\x1b[B"),
                F1 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2P"),
                F1 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[1;3P"),
                F1 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5P"),
                F1 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;6P"),
                F1 => Event::StrInput("\x1bOP"),
                F2 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2Q"),
                F2 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[1;3Q"),
                F2 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5Q"),
                F2 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;6Q"),
                F2 => Event::StrInput("\x1bOQ"),
                F3 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2R"),
                F3 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[1;3R"),
                F3 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5R"),
                F3 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;6R"),
                F3 => Event::StrInput("\x1bOR"),
                F4 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[1;2S"),
                F4 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[1;3S"),
                F4 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[1;5S"),
                F4 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[1;6S"),
                F4 => Event::StrInput("\x1bOS"),
                F5 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[15;2~"),
                F5 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[15;3~"),
                F5 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[15;5~"),
                F5 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[15;6~"),
                F5 => Event::StrInput("\x1b[15~"),
                F6 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[17;2~"),
                F6 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[17;3~"),
                F6 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[17;5~"),
                F6 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[17;6~"),
                F6 => Event::StrInput("\x1b[17~"),
                F7 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[18;2~"),
                F7 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[18;3~"),
                F7 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[18;5~"),
                F7 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[18;6~"),
                F7 => Event::StrInput("\x1b[18~"),
                F8 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[19;2~"),
                F8 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[19;3~"),
                F8 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[19;5~"),
                F8 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[19;6~"),
                F8 => Event::StrInput("\x1b[19~"),
                F9 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[20;2~"),
                F9 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[20;3~"),
                F9 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[20;5~"),
                F9 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[20;6~"),
                F9 => Event::StrInput("\x1b[20~"),
                F10 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[21;2~"),
                F10 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[21;3~"),
                F10 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[21;5~"),
                F10 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[21;6~"),
                F10 => Event::StrInput("\x1b[21~"),
                F11 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[23;2~"),
                F11 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[23;3~"),
                F11 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[23;5~"),
                F11 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[23;6~"),
                F11 => Event::StrInput("\x1b[23~"),
                F12 if mods.contains(Mod::SHIFT_MASK) => Event::StrInput("\x1b[24;2~"),
                F12 if mods.contains(Mod::SUPER_MASK) => Event::StrInput("\x1b[24;3~"),
                F12 if mods.contains(Mod::CONTROL_MASK) => Event::StrInput("\x1b[24;5~"),
                F12 if mods.contains(Mod::META_MASK) => Event::StrInput("\x1b[24;6~"),
                F12 => Event::StrInput("\x1b[24~"),
                Super_L | Super_R | Hyper_L | Hyper_R | Control_L | Control_R |
                    Alt_L | Alt_R | Meta_L | Meta_R | Shift_L | Shift_R |
                    Caps_Lock | Scroll_Lock | Shift_Lock | ModeLock => Event::Blank,
                _ => Event::CharInput(gdk::keyval_to_unicode(kv).unwrap_or(kv as u8 as char), IsControlHeld(mods.contains(Mod::CONTROL_MASK))),
            });
        }
        glarea.queue_draw();
        Inhibit(kv == gdk::enums::key::Tab) // prevent tab from switching focus to the top bar
    }));

    glarea.connect_key_release_event(clone!(im => move |_glarea, event| {
        let _ = im.filter_keypress(event);
        Inhibit(true)
    }));

    im.connect_commit(clone!(glarea, state => move |_im, s| {
        trace!("IM input: str {:?}", s);
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(Event::StringInput(s.to_owned()));
        }
        glarea.queue_draw();
    }));

    glarea.drag_dest_set(gtk::DestDefaults::ALL, &[], gdk::DragAction::COPY);
    glarea.drag_dest_add_text_targets();
    glarea.drag_dest_add_uri_targets();

    glarea.connect_drag_data_received(clone!(state => move |_glarea, _dctx, _x, _y, data, _info, _time| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            let uris = data.get_uris();
            if uris.len() > 0 {
                state.event_queue.push(Event::StringInput(uris.iter().map(|u| u.trim().replace("file://", "")).collect::<Vec<_>>().join(" ")));
            } else if let Some(text) = data.get_text() {
                state.event_queue.push(Event::StringInput(text.replace("file://", "").trim().to_owned()));
            }
        }
    }));

    glarea.connect_property_scale_factor_notify(clone!(state => move |glarea| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(Event::HiDPIFactorChanged(glarea.get_scale_factor() as f32));
        }
        glarea.queue_draw();
    }));

    glarea.set_can_focus(true);
    glarea.connect_focus_in_event(clone!(im => move |_glarea, _event| {
        im.focus_in();
        Inhibit(false)
    }));
    glarea.connect_focus_out_event(clone!(im => move |_glarea, _event| {
        im.focus_out();
        Inhibit(false)
    }));
    glarea.grab_focus();

    GLOBAL.with(clone!(glarea => move |global| {
        // NOTE: important to store glarea somewhere, adding to window doesn't prevent from
        // being dropped at the end of the scope https://github.com/gtk-rs/gtk/issues/637
        // (conveniently, we need to store it for the notifier here)
        *global.borrow_mut() = Some(glarea);
    }));

    (glarea, state)
}

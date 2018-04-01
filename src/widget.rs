use std::ptr;
use std::rc::Rc;
use std::sync::Arc;
use std::cell::RefCell;
// use std::thread::JoinHandle;

use epoxy;
use shared_library::dynamic_library::DynamicLibrary;

use glib;
use gdk;
use gtk;
use gtk::prelude::*;

use alacritty::{cli, gl};
use alacritty::display::{Display, InitialSize};
use alacritty::event_loop::{self, EventLoop, WindowNotifier};
use alacritty::tty::{self, Pty};
use alacritty::sync::FairMutex;
use alacritty::term::Term;
use alacritty::config::Config;

// TODO vec for multiple widgets
thread_local!{
    static GLOBAL: RefCell<Option<gtk::GLArea>> = RefCell::new(None);
}

pub enum Event {
    CharInput(char),
    WindowResized(u32, u32),
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

    let state: Rc<RefCell<Option<State>>> = Rc::new(RefCell::new(None));

    glarea.connect_realize(clone!(state => move |glarea| {
        let mut state = state.borrow_mut();
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
            2.0 // XXX gtk returns 1 at first, change isn't handled // glarea.get_scale_factor() as f32
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

    glarea.connect_render(clone!(state => move |_glarea, _glctx| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            let mut terminal = state.terminal.lock();
            for event in state.event_queue.drain(..) {
                match event {
                    Event::CharInput(c) => {
                        let len = c.len_utf8();
                        let mut bytes = Vec::with_capacity(len);
                        unsafe {
                            bytes.set_len(len);
                            c.encode_utf8(&mut bytes[..]);
                        }
                        use alacritty::event::Notify;
                        state.loop_notifier.notify(bytes);
                    },
                    Event::WindowResized(w, h) => {
                        state.display.resize_channel().send((w, h)).expect("send new size");
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

    glarea.connect_key_press_event(clone!(state => move |glarea, event| {
        let kv = event.get_keyval();
        println!("KEY {:?}", kv);
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(Event::CharInput(kv as u8 as char));
        }
        glarea.queue_draw();
        Inhibit(false)
    }));

    glarea.connect_property_scale_factor_notify(clone!(state => move |glarea| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            // state.event_queue.push(Event::HiDPIFactorChanged(glarea.get_scale_factor() as f32));
        }
        glarea.queue_draw();
    }));

    glarea.set_can_focus(true);
    glarea.grab_focus();

    GLOBAL.with(clone!(glarea => move |global| {
        // NOTE: important to store glarea somewhere, adding to window doesn't prevent from
        // being dropped at the end of the scope https://github.com/gtk-rs/gtk/issues/637
        // (conveniently, we need to store it for the notifier here)
        *global.borrow_mut() = Some(glarea);
    }));

    (glarea, state)
}

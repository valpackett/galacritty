use std::{ptr, mem};
use std::rc::Rc;
use std::sync::Arc;
use std::cell::RefCell;
// use std::thread::JoinHandle;

use epoxy;
use shared_library::dynamic_library::DynamicLibrary;
use glutin; // should eventually disappear

use glib;
use gdk;
use gtk;
use gtk::prelude::*;

use alacritty::{cli, gl};
use alacritty::event::{Processor, DummyWindowLoop};
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

struct State {
    config: Config,
    display: Display,
    terminal: Arc<FairMutex<Term>>,
    pty: Pty,
    processor: Processor<event_loop::Notifier>,
    // io_thread: JoinHandle<(EventLoop<fs::File>, event_loop::State)>,
    event_queue: Vec<glutin::Event>,
}

/// Creates a GLArea that runs an Alacritty terminal emulator.
///
/// Eventually should be a GObject subclass, usable outside of Rust.
pub fn alacritty_widget() -> gtk::GLArea {
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

        //let loop_tx = event_loop.channel();

        let processor = Processor::new(
            event_loop::Notifier(event_loop.channel()),
            display.resize_channel(),
            &options,
            &config,
            options.ref_test,
            display.size().to_owned(),
        );

        let _io_thread = event_loop.spawn(None);

        *state = Some(State {
            config, display, terminal, pty, processor, //io_thread,
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
            let mut terminal = state.processor.process_events_push::<DummyWindowLoop>(&state.terminal, &mut state.event_queue, None);
            if terminal.needs_draw() {
                state.display.handle_resize(&mut terminal, &state.config, &mut [&mut state.pty, &mut state.processor]);
                state.display.draw(terminal, &state.config, state.processor.selection.as_ref(), true);
            }
        }
        Inhibit(false)
    }));

    glarea.connect_resize(clone!(state => move |glarea, w, h| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(glutin::Event::WindowEvent {
                window_id: unsafe { mem::transmute::<(u64, u64), glutin::WindowId>((0, 0)) },
                event: glutin::WindowEvent::Resized(w as u32, h as u32)
            });
        }
        glarea.queue_draw();
    }));

    glarea.add_events(gdk::EventMask::KEY_PRESS_MASK.bits() as i32);

    glarea.connect_key_press_event(clone!(state => move |glarea, event| {
        let kv = event.get_keyval();
        println!("KEY {:?}", kv);
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(glutin::Event::WindowEvent {
                window_id: unsafe { mem::transmute::<(u64, u64), glutin::WindowId>((0, 0)) },
                event: /*glutin::WindowEvent::KeyboardInput {
                    device_id: unsafe { mem::transmute::<u64, glutin::DeviceId>(0) },
                    input: glutin::KeyboardInput {
                        scancode: kv,
                        state: glutin::ElementState::Pressed,
                        virtual_keycode: Some(glutin::VirtualKeyCode::A),
                        modifiers: glutin::ModifiersState::default()
                    }
                }*/ glutin::WindowEvent::ReceivedCharacter(kv as u8 as char)
            });
        }
        glarea.queue_draw();
        Inhibit(false)
    }));

    glarea.connect_property_scale_factor_notify(clone!(state => move |glarea| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(glutin::Event::WindowEvent {
                window_id: unsafe { mem::transmute::<(u64, u64), glutin::WindowId>((0, 0)) },
                event: glutin::WindowEvent::HiDPIFactorChanged(glarea.get_scale_factor() as f32)
            });
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

    glarea
}

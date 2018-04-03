extern crate gtk;
extern crate glib;
extern crate gio;
extern crate gdk;
extern crate epoxy;
extern crate shared_library;
extern crate alacritty;
#[macro_use]
extern crate log;

use std::env::args;
use std::rc::Rc;
use std::cell::RefCell;

use gio::prelude::*;
use gio::{Menu, MenuExt, MenuItem, SimpleAction};
use gtk::prelude::*;

#[macro_use]
pub mod util; // order matters for macros
pub mod widget;

fn build_actions(app: gtk::Application,
                 window: gtk::ApplicationWindow,
                 clipboard: gtk::Clipboard,
                 glarea: gtk::GLArea,
                 state: Rc<RefCell<Option<widget::State>>>) {
    let about_action = SimpleAction::new("HelpAbout", None);
    about_action.connect_activate(clone!(window => move |_, _| {
        let about = gtk::AboutDialog::new();
        about.set_transient_for(Some(&window));
        about.set_program_name("Galacritty");
        about.set_version(env!("CARGO_PKG_VERSION"));
        about.set_logo_icon_name("technology.unrelenting.galacritty");
        about.set_authors(&[env!("CARGO_PKG_AUTHORS")]);
        about.set_comments(env!("CARGO_PKG_DESCRIPTION"));
        about.connect_response(|about, _| about.destroy());
        about.show();
    }));
    app.add_action(&about_action);

    let paste_action = SimpleAction::new("Paste", None);
    paste_action.connect_activate(clone!(glarea, state => move |_, _| {
        if let Some(text) = clipboard.wait_for_text() {
            let mut state = state.borrow_mut();
            if let Some(ref mut state) = *state {
                // TODO: bracketed paste support
                state.event_queue.push(widget::Event::StringInput(text.replace("\r\n", "\r").replace("\n", "\r")));
            }
            glarea.queue_draw();
        }
    }));
    window.add_action(&paste_action);
    app.set_accels_for_action("win.Paste", &["<Control><Shift>v", "<Shift>Insert"]);

    let font_decr_action = SimpleAction::new("FontDecrease", None);
    font_decr_action.connect_activate(clone!(glarea, state => move |_, _| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ChangeFontSize(-1));
        }
        glarea.queue_draw();
    }));
    window.add_action(&font_decr_action);
    app.set_accels_for_action("win.FontDecrease", &["<Control>minus", "<Control>KP_Subtract"]);

    let font_incr_action = SimpleAction::new("FontIncrease", None);
    font_incr_action.connect_activate(clone!(glarea, state => move |_, _| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ChangeFontSize(1));
        }
        glarea.queue_draw();
    }));
    window.add_action(&font_incr_action);
    app.set_accels_for_action("win.FontIncrease", &["<Control>equal", "<Control>plus", "<Control>KP_Add"]);

    let font_reset_action = SimpleAction::new("FontReset", None);
    font_reset_action.connect_activate(move |_, _| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ResetFontSize);
        }
        glarea.queue_draw();
    });
    window.add_action(&font_reset_action);
    app.set_accels_for_action("win.FontReset", &["<Control>0", "<Control>KP_0"]);
}

fn build_main_menu() -> Menu {
    let menu = Menu::new();

    let section = Menu::new();
    section.append_item(&MenuItem::new("About", "app.HelpAbout"));
    menu.append_section(None, &section);

    menu.freeze();
    menu
}

fn build_header_bar() -> gtk::HeaderBar {
    let header_bar = gtk::HeaderBar::new();

    let font_decr_btn = gtk::Button::new_from_icon_name("zoom-out-symbolic", gtk::IconSize::SmallToolbar.into());
    font_decr_btn.set_can_focus(false);
    font_decr_btn.set_tooltip_text("Decrease font size");
    font_decr_btn.set_action_name("win.FontDecrease");
    header_bar.pack_start(&font_decr_btn);

    let font_incr_btn = gtk::Button::new_from_icon_name("zoom-in-symbolic", gtk::IconSize::SmallToolbar.into());
    font_incr_btn.set_can_focus(false);
    font_incr_btn.set_tooltip_text("Increase font size");
    font_incr_btn.set_action_name("win.FontIncrease");
    header_bar.pack_start(&font_incr_btn);

    let paste_btn = gtk::Button::new_from_icon_name("edit-paste-symbolic", gtk::IconSize::SmallToolbar.into());
    paste_btn.set_can_focus(false);
    paste_btn.set_tooltip_text("Paste from clipboard");
    paste_btn.set_action_name("win.Paste");
    header_bar.pack_end(&paste_btn);

    header_bar.set_show_close_button(true);
    header_bar
}

fn build_ui(app: &gtk::Application) {
    gtk::Window::set_default_icon_name("technology.unrelenting.galacritty");

    let window = gtk::ApplicationWindow::new(app);

    window.set_title("Galacritty");
    window.set_border_width(0);
    window.set_default_size(1280, 720);

    window.connect_delete_event(clone!(window => move |_, _| {
        window.destroy();
        Inhibit(false)
    }));

    let clipboard = gtk::Clipboard::get(&gdk::Atom::intern("CLIPBOARD"));

    let header_bar = build_header_bar();
    window.set_titlebar(Some(&header_bar));

    let (glarea, state) = widget::alacritty_widget(window.clone(), header_bar);

    build_actions(app.clone(), window.clone(), clipboard, glarea.clone(), state.clone());

    app.set_app_menu(Some(&build_main_menu()));
    window.add(&glarea);
    window.show_all();
}

fn main() {
    let _ = alacritty::logging::initialize(&alacritty::cli::Options::default());

    let application = gtk::Application::new(
        "technology.unrelenting.galacritty",
        gio::ApplicationFlags::empty()
    ).expect("gtk::Application::new");

    application.connect_startup(|app| {
        build_ui(app);
    });
    application.connect_activate(|_| {});

    application.run(&args().collect::<Vec<_>>());
}

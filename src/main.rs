extern crate gtk;
extern crate glib;
extern crate gio;
extern crate gdk;
extern crate epoxy;
extern crate shared_library;
extern crate glutin;
extern crate alacritty;

use std::env::args;
use std::rc::Rc;
use std::cell::RefCell;

use gio::prelude::*;
use gio::{Menu, MenuExt, MenuItem, SimpleAction};
use gtk::prelude::*;

#[macro_use]
pub mod util; // order matters for macros
pub mod widget;

fn build_about_action(window: gtk::ApplicationWindow) -> SimpleAction {
    let about_action = SimpleAction::new("HelpAbout", None);
    about_action.connect_activate(move |_, _| {
        let about = gtk::AboutDialog::new();
        about.set_transient_for(Some(&window));
        about.set_program_name("Galacritty");
        about.set_version(env!("CARGO_PKG_VERSION"));
        about.set_logo_icon_name("technology.unrelenting.galacritty");
        about.set_authors(&[env!("CARGO_PKG_AUTHORS")]);
        about.set_comments(env!("CARGO_PKG_DESCRIPTION"));
        about.connect_response(|about, _| about.destroy());
        about.show();
    });
    about_action.set_enabled(true);
    about_action
}

fn build_main_menu() -> Menu {
    let menu = Menu::new();

    let section = Menu::new();
    section.append_item(&MenuItem::new("About", "app.HelpAbout"));
    menu.append_section(None, &section);

    menu.freeze();
    menu
}

fn configure_header_bar(header_bar: &mut gtk::HeaderBar, glarea: gtk::GLArea, state: Rc<RefCell<Option<widget::State>>>) {
    let font_decr_btn = gtk::Button::new_from_icon_name("list-remove-symbolic", gtk::IconSize::SmallToolbar.into());
    font_decr_btn.set_can_focus(false);
    font_decr_btn.set_tooltip_text("Decrease font size");
    font_decr_btn.connect_clicked(clone!(state, glarea => move |_| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ChangeFontSize(-1));
        }
        glarea.queue_draw();
    }));
    header_bar.pack_start(&font_decr_btn);

    let font_incr_btn = gtk::Button::new_from_icon_name("list-add-symbolic", gtk::IconSize::SmallToolbar.into());
    font_incr_btn.set_can_focus(false);
    font_incr_btn.set_tooltip_text("Increase font size");
    font_incr_btn.connect_clicked(move |_| {
        let mut state = state.borrow_mut();
        if let Some(ref mut state) = *state {
            state.event_queue.push(widget::Event::ChangeFontSize(1));
        }
        glarea.queue_draw();
    });
    header_bar.pack_start(&font_incr_btn);

    header_bar.set_show_close_button(true);
}

fn build_ui(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(app);

    window.set_title("Galacritty");
    window.set_border_width(0);
    window.set_default_size(1280, 720);

    window.connect_delete_event(clone!(window => move |_, _| {
        window.destroy();
        Inhibit(false)
    }));

    let mut header_bar = gtk::HeaderBar::new();
    let (glarea, state) = widget::alacritty_widget(header_bar.clone());
    configure_header_bar(&mut header_bar, glarea.clone(), state.clone());

    app.add_action(&build_about_action(window.clone()));
    app.set_app_menu(Some(&build_main_menu()));
    window.set_titlebar(Some(&header_bar));

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

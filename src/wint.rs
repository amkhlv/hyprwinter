extern crate dirs;
extern crate gdk;
extern crate gdk_sys;
extern crate gio;
extern crate glib;
extern crate gtk;

use crate::gdk::prelude::{ApplicationExt, ApplicationExtManual};
use glib::clone;
use glib::signal::Propagation;
use gtk::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use hyprwinter::{
    check_css, check_tilings, get_conf, get_config_dir, get_wm_data, hypr_dispatch, make_vbox,
    Config, Monitor, Window,
};

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_xml_rs;

#[derive(Debug, Deserialize)]
struct WindowSimple {
    #[serde(rename = "@nick", default)]
    pub nick: String,
    #[serde(rename = "@geometry", default)]
    pub geometry: String,
}

#[derive(Debug, Deserialize)]
struct Display {
    #[serde(rename = "@resolution", default)]
    pub resolution: String,

    #[serde(rename = "window", default)]
    pub windows: Vec<WindowSimple>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename = "displays", default)]
struct Displays {
    #[serde(rename = "display", default)]
    pub items: Vec<Display>,
}

fn get_geometry(xml_path: &PathBuf, nick: String, geom: &String) -> Option<Vec<u32>> {
    let tilings: Displays = serde_xml_rs::from_reader(File::open(xml_path).unwrap()).unwrap();
    tilings
        .items
        .iter()
        .filter(|disp| &disp.resolution == geom)
        .next()
        .and_then(|x| {
            x.windows
                .iter()
                .filter(|w| w.nick == nick)
                .next()
                .map(|ni| {
                    ni.geometry
                        .split(",")
                        .map(|s| str::parse::<u32>(s).unwrap())
                        .collect()
                })
        })
}

fn do_resize(wid: Window, g: &Vec<u32>, geom: &Monitor) {
    let address = format!("address:0x{:x}", wid);
    let x = (g[0] as f32 / geom.scale).round();
    let y = (g[1] as f32 / geom.scale).round();
    let width = (g[2] as f32 / geom.scale).round();
    let height = (g[3] as f32 / geom.scale).round();

    hypr_dispatch("focuswindow", &address);
    hypr_dispatch("setfloating", &address);

    thread::sleep(Duration::from_millis(100));

    hypr_dispatch(
        "resizewindowpixel",
        format!("exact {} {},{}", width, height, address),
    );
    hypr_dispatch(
        "movewindowpixel",
        format!("exact {} {},{}", x, y, address),
    );
    hypr_dispatch("alterzorder", format!("top,{}", address));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = get_config_dir();
    let conf: Config = get_conf().expect("Could not read the configuration file");
    let maxlen = conf.maxwidth;
    let blacklist = Rc::new(conf.blacklist);
    let space_between_buttons = conf.space_between_buttons;
    let (wins, geom, desktop, active) = get_wm_data();

    let application = gtk::Application::builder()
        .application_id("com.andreimikhailov.winterreise")
        .build();
    let xml_path = Path::join(&config_dir, "tilings.xml");
    check_tilings(&xml_path);
    let css = Path::join(&config_dir, "style.css");
    check_css(&css);
    let xml_path = Rc::new(xml_path);
    application.connect_activate(move |app| {
        let provider = gtk::CssProvider::new();
        match css.to_str() {
            Some(x) => match provider.load_from_path(x) {
                Ok(_) => (),
                Err(x) => {
                    println!("ERROR: {:?}", x);
                }
            },
            None => {
                println!("ERROR: path contains non-unicode characters");
            }
        };
        let screen = gdk::Screen::default();
        match screen {
            Some(scr) => {
                gtk::StyleContext::add_provider_for_screen(&scr, &provider, 799);
            }
            _ => (),
        };
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("Tile");
        window.set_type_hint(gdk::WindowTypeHint::Dialog);
        window.style_context().add_class("main_window_tile");
        window.connect_key_press_event(
            clone!(@weak app => @default-return Propagation::Proceed, move |_w,e| {
                let keyval = e.keyval();
                let _keystate = e.state();
                if *keyval == gdk_sys::GDK_KEY_Escape as u32 {
                    app.quit();
                    return Propagation::Stop;
                } else { return Propagation::Proceed; }
            }),
        );

        let (vbox, charhints) = make_vbox(
            &wins,
            Some(desktop),
            space_between_buttons,
            maxlen,
            &blacklist,
            &active,
        );
        window.add(&vbox);
        let entry = gtk::Entry::new();
        entry.style_context().add_class("wmjump_cmd_entry");
        let geom1 = Rc::clone(&geom);
        let xml_path = Rc::clone(&xml_path);
        entry.connect_activate(clone!(@weak entry, @weak app => move |_| {
            let command : String = entry.text().to_string();
            let tilings : Vec<(Window, Option<Vec<u32>>)> = command
                .split_whitespace()
                .filter_map(|com| {
                    let mut it = com.chars();
                    let charhint = it.next()?;
                    let hint = (charhint as u8).checked_sub(97)?;
                    let wid = *charhints.get(&hint)?;
                    let tiling = it.collect::<String>();
                    let mg = get_geometry(
                        &xml_path,
                        tiling,
                        &format!("{}x{}", geom1.width, geom1.height),
                    );
                    Some((wid, mg))
                })
                .collect();
            app.quit();
            for (wid, mg) in tilings.iter() {
                match mg {
                    Some(g) => do_resize(*wid, &g, &geom1),
                    None => println!("No geometry found for window {} at screen resolution {}", wid, format!("{}x{}", geom1.width, geom1.height))
                }
            }
        }));
        vbox.add(&entry);
        entry.grab_focus();
        window.show_all();
    });
    application.run();
    Ok(())
}

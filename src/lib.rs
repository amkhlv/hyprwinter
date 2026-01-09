#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_xml_rs;

use dirs::home_dir;
use gtk::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub enum WintError {
    //Errors from external libs:
    SerDe(serde_xml_rs::Error),
    NoConfigFile(std::io::Error),
}

impl std::fmt::Display for WintError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            WintError::SerDe(ref err) => err.fmt(f),
            WintError::NoConfigFile(ref err) => err.fmt(f),
        }
    }
}
impl std::error::Error for WintError {}
impl std::convert::From<serde_xml_rs::Error> for WintError {
    fn from(err: serde_xml_rs::Error) -> WintError {
        WintError::SerDe(err)
    }
}
impl std::convert::From<std::io::Error> for WintError {
    fn from(err: std::io::Error) -> WintError {
        WintError::NoConfigFile(err)
    }
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct BlacklistedItem {
    pub class: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct BlacklistedItems {
    pub item: Vec<BlacklistedItem>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum TMPFile {
    #[serde(rename = "in_xdg_runtime")]
    InXdgRuntime,
    #[serde(rename = "in_tmp")]
    InTmp,
    #[serde(rename = "custom")]
    Custom(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename = "configuration")]
pub struct Config {
    pub tmpfile: TMPFile,
    #[serde(rename = "spaceBetweenButtons", default)]
    pub space_between_buttons: i32,
    pub maxwidth: usize,
    pub blacklist: BlacklistedItems,
}

pub struct WM {
    pub wins: Rc<Vec<(u32, u32, String, String)>>,
    pub desktop: u32,
}

pub type Window = u64;

pub fn get_wm_data() -> (
    Rc<Vec<(Window, u32, String, String)>>,
    Rc<String>,
    u32,
    Window,
) {
    return (Rc::new(Vec::new()), Rc::new("TODO".to_string()), 0, 0);
}

pub fn abbreviate(x: String, maxlen: usize) -> String {
    let chars = x.chars().collect::<Vec<_>>();
    let len = chars.len();
    if len < maxlen {
        return x;
    } else {
        return format!(
            "{}...{}",
            &chars[..(maxlen / 8) * 4]
                .iter()
                .cloned()
                .collect::<String>(),
            &chars[(len - (maxlen / 8) * 4)..len]
                .iter()
                .cloned()
                .collect::<String>()
        );
    }
}
pub fn make_vbox(
    wins: &Rc<Vec<(Window, u32, String, String)>>,
    desktop: Option<u32>,
    space_between_buttons: i32,
    maxlen: usize,
    blacklist: &Rc<BlacklistedItems>,
    active: &Window,
) -> (gtk::Box, HashMap<u8, Window>) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, space_between_buttons);
    vbox.style_context().add_class("main_vbox");
    let mut charhints: HashMap<u8, Window> = HashMap::new();
    let mut j = 0 as u8;
    match desktop {
        Some(d) => println!("only showing windows on desktop {}", d),
        None => println!("showing windows on all desktops"),
    }
    for (num, win_desktop, name, class) in (*wins)
        .iter()
        .filter(|win| match desktop {
            Some(d) => d == win.1,
            None => true,
        })
        .filter(|win| {
            !(*blacklist)
                .item
                .iter()
                .map(|i| &i.class)
                .collect::<Vec<&String>>()
                .contains(&&win.3)
        })
    {
        let class_sanitized = class.replace(".", "_");
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, space_between_buttons);
        let lbtn = gtk::Button::new();
        let llbl = gtk::Label::new(Some(&format!("{}", (j + 97) as char)));
        if num == active {
            lbtn.style_context().add_class("wmjump_lbtn_current");
        } else {
            lbtn.style_context()
                .add_class(&["wbtn_", &class_sanitized].concat()[..]);
            lbtn.style_context().add_class("wmjump_lbtn");
        }
        lbtn.add(&llbl);
        let rbtn = gtk::Button::new();
        let rlbl = gtk::Label::new(Some(&format!("{}", (j + 97) as char)));
        if num == active {
            rbtn.style_context().add_class("wmjump_rbtn_current");
        } else {
            rbtn.style_context()
                .add_class(&["wbtn_", &class_sanitized].concat()[..]);
            rbtn.style_context().add_class("wmjump_rbtn");
        }
        rbtn.add(&rlbl);
        let btn = gtk::Button::new();
        let truncated = name.clone();
        let lbl = gtk::Label::new(Some(&format!(
            "{}: {}",
            win_desktop + 1,
            abbreviate(truncated, maxlen)
        )));
        btn.style_context()
            .add_class(&["wbtn_", &class_sanitized].concat()[..]);
        btn.style_context().add_class("wmjump_button");
        btn.add(&lbl);
        hbox.add(&lbtn);
        hbox.add(&btn);
        hbox.add(&rbtn);
        vbox.add(&hbox);
        charhints.insert(j, *num);
        j += 1;
    }
    return (vbox, charhints);
}

pub fn get_config_dir() -> PathBuf {
    let p = Path::join(Path::new(&home_dir().unwrap()), ".config/winterreise/");
    if !p.exists() {
        std::fs::create_dir(&p).expect("Could not create config directory");
    }
    p
}
pub fn get_conf() -> Result<Config, WintError> {
    let config_dir = get_config_dir();
    let config_file_path = Path::join(&config_dir, "config.xml");
    if !config_file_path.exists() {
        let init_config = include_str!("config/config.xml");
        std::fs::write(&config_file_path, init_config)
            .expect("Could not write default config file");
    }
    let config_file = File::open(config_file_path)?;
    let conf = serde_xml_rs::from_reader(config_file)?;
    return Ok(conf);
}
pub fn check_css(p: &Path) -> () {
    if !p.exists() {
        let init_css = include_str!("config/style.css");
        std::fs::write(p, init_css).expect("Could not write default css file");
    }
}
pub fn check_tilings(p: &Path) -> () {
    if !p.exists() {
        let init_css = include_str!("config/tilings.xml");
        std::fs::write(p, init_css).expect("Could not write default tilings file");
    }
}

pub fn go_to_window(win: Window) {
    println!("-- going to window {:?}\n   ...", win);
}

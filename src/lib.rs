#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_xml_rs;
use dirs::home_dir;
use gtk::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
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
fn parse_hex_to_u64(hex_str: &str) -> Result<u64, std::num::ParseIntError> {
    // Ensure the string starts with "0x" and strip it
    let trimmed = hex_str.trim_start_matches("0x");
    // Parse the remaining part as a hexadecimal number
    u64::from_str_radix(trimmed, 16)
}
pub fn get_wm_data() -> (
    Rc<Vec<(Window, u32, String, String)>>,
    Rc<String>,
    u32,
    Window,
) {
    // Run the "hyprctl -j monitors" command
    let output = Command::new("hyprctl")
        .arg("-j")
        .arg("monitors")
        .output()
        .expect("Failed to run hyprctl command");

    // Convert output to string
    let json_str = String::from_utf8(output.stdout).expect("Failed to parse output as UTF-8");

    // Parse the JSON
    let monitors: Value = serde_json::from_str(&json_str).expect("Failed to parse JSON output");
    let geom = match monitors {
        Value::Array(ref arr) => {
            let monitor = arr.get(0).expect("Expected at least one monitor");
            let width = monitor["width"].as_u64().unwrap() as u32;
            let height = monitor["height"].as_u64().unwrap() as u32;
            (width, height)
        }
        _ => panic!("Unexpected JSON format"),
    };

    // Run the "hyprctl -j clients" command
    let output = Command::new("hyprctl")
        .arg("-j")
        .arg("clients")
        .output()
        .expect("Failed to run hyprctl command");

    // Convert output to string
    let json_str = String::from_utf8(output.stdout).expect("Failed to parse output as UTF-8");

    // Parse the JSON
    let clients: Value = serde_json::from_str(&json_str).expect("Failed to parse JSON output");
    // Extract wins (address, workspace.id, title, class)
    let wins = clients
        .as_array()
        .expect("Expected JSON array")
        .iter()
        .filter_map(|client| {
            println!("Client: {:?}", client);
            let address = parse_hex_to_u64(client["address"].as_str()?).unwrap();
            let workspace_id = client["workspace"]["id"].as_u64()? as u32;
            let title = client["title"].as_str()?.to_string();
            let class = client["class"].as_str()?.to_string();
            Some((address, workspace_id, title, class))
        })
        .collect::<Vec<_>>();

    let output = Command::new("hyprctl")
        .arg("-j")
        .arg("activeworkspace")
        .output();

    let cur_desktop = if let Ok(output) = output {
        if let Ok(json_str) = String::from_utf8(output.stdout) {
            if let Ok(workspace) = serde_json::from_str::<Value>(&json_str) {
                if let Some(id) = workspace["id"].as_u64() {
                    id as u32
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };

    let output = Command::new("hyprctl")
        .arg("-j")
        .arg("activewindow")
        .output();

    let cur_window = if let Ok(output) = output {
        if let Ok(json_str) = String::from_utf8(output.stdout) {
            if let Ok(window) = serde_json::from_str::<Value>(&json_str) {
                if let Some(address) = window["address"].as_str() {
                    parse_hex_to_u64(address).unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };

    (
        Rc::new(wins),
        Rc::new(format!("{}x{}", geom.0, geom.1)),
        cur_desktop,
        cur_window,
    )
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
            win_desktop,
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
    println!("-- going to window {:x}\n   ...", win);
    let jumper = Command::new("hyprctl")
        .arg("dispatch")
        .arg("focuswindow")
        .arg(format!("address:0x{:x}", win))
        .output()
        .expect("Failed to run hyprctl command");

    if jumper.status.success() {
        println!(
            "Command succeeded: {}",
            String::from_utf8_lossy(&jumper.stdout)
        );
    } else {
        eprintln!(
            "Command failed with status {}: {}",
            jumper.status,
            String::from_utf8_lossy(&jumper.stderr)
        );
    }
}

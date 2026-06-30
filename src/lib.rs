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
use std::thread;
use std::time::Duration;

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

#[derive(Debug)]
pub struct Monitor {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
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
    pub wins: Rc<Vec<(Window, Workspace, String, String)>>,
    pub desktop: Workspace,
}

pub type Window = u64;
pub type Workspace = i64;

fn parse_hex_to_u64(hex_str: &str) -> Result<u64, std::num::ParseIntError> {
    let trimmed = hex_str
        .trim()
        .trim_start_matches("address:")
        .trim_start_matches("0x");
    // Parse the remaining part as a hexadecimal number
    u64::from_str_radix(trimmed, 16)
}

fn run_hyprctl_json(args: &[&str]) -> Option<Value> {
    let output = Command::new("hyprctl").args(args).output().ok()?;
    if !output.status.success() {
        eprintln!(
            "hyprctl {} failed with status {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    serde_json::from_str(&stdout)
        .map_err(|err| {
            eprintln!("hyprctl {} returned invalid JSON: {}", args.join(" "), err);
            err
        })
        .ok()
}

fn json_to_workspace_id(value: &Value) -> Option<Workspace> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|id| Workspace::try_from(id).ok()))
}

fn json_to_window_address(value: &Value) -> Option<Window> {
    if let Some(address) = value.as_str() {
        parse_hex_to_u64(address).ok()
    } else {
        value.as_u64()
    }
}

fn lua_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

fn lua_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.parse::<i64>().is_ok() {
        trimmed.to_string()
    } else {
        lua_string(trimmed)
    }
}

fn lua_dispatcher_expr(dispatcher: &str, argument: &str) -> Option<String> {
    let argument = argument.trim();
    match dispatcher {
        "workspace" => Some(format!(
            "hl.dsp.focus({{ workspace = {} }})",
            lua_value(argument)
        )),
        "focuswindow" => Some(format!(
            "hl.dsp.focus({{ window = {} }})",
            lua_string(argument)
        )),
        "setfloating" => {
            if argument.is_empty() {
                Some("hl.dsp.window.float({ action = \"set\" })".to_string())
            } else {
                Some(format!(
                    "hl.dsp.window.float({{ action = \"set\", window = {} }})",
                    lua_string(argument)
                ))
            }
        }
        "alterzorder" => {
            let mut args = argument.splitn(2, ',');
            let mode = args.next()?.trim();
            let window = args.next().map(str::trim).filter(|s| !s.is_empty());

            Some(match window {
                Some(window) => format!(
                    "hl.dsp.window.alter_zorder({{ mode = {}, window = {} }})",
                    lua_string(mode),
                    lua_string(window)
                ),
                None => format!(
                    "hl.dsp.window.alter_zorder({{ mode = {} }})",
                    lua_string(mode)
                ),
            })
        }
        "resizewindowpixel" | "movewindowpixel" => {
            let call = if dispatcher == "resizewindowpixel" {
                "hl.dsp.window.resize"
            } else {
                "hl.dsp.window.move"
            };
            let mut args = argument.splitn(2, ',');
            let geometry = args.next()?.trim();
            let window = args.next().map(str::trim).filter(|s| !s.is_empty());
            let geometry = geometry.strip_prefix("exact ").unwrap_or(geometry);
            let mut parts = geometry.split_whitespace();
            let x = parts.next()?;
            let y = parts.next()?;
            if parts.next().is_some() {
                return None;
            }

            Some(match window {
                Some(window) => format!(
                    "{}({{ x = {}, y = {}, window = {} }})",
                    call,
                    x,
                    y,
                    lua_string(window)
                ),
                None => format!("{}({{ x = {}, y = {} }})", call, x, y),
            })
        }
        _ => None,
    }
}

fn run_hypr_dispatch(args: &[&str]) -> Result<(), String> {
    let output = Command::new("hyprctl")
        .arg("dispatch")
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "status {}: {}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub fn hypr_dispatch(dispatcher: &str, argument: impl AsRef<str>) -> bool {
    let argument = argument.as_ref();

    if let Some(lua_expr) = lua_dispatcher_expr(dispatcher, argument) {
        match run_hypr_dispatch(&[&lua_expr]) {
            Ok(()) => return true,
            Err(lua_error) => match run_hypr_dispatch(&[dispatcher, argument]) {
                Ok(()) => return true,
                Err(legacy_error) => {
                    eprintln!(
                        "hyprctl dispatch failed\n  lua: hyprctl dispatch {} -> {}\n  legacy: hyprctl dispatch {} {} -> {}",
                        lua_expr, lua_error, dispatcher, argument, legacy_error
                    );
                    return false;
                }
            },
        }
    }

    match run_hypr_dispatch(&[dispatcher, argument]) {
        Ok(()) => true,
        Err(err) => {
            eprintln!(
                "hyprctl dispatch {} {} failed: {}",
                dispatcher, argument, err
            );
            false
        }
    }
}

pub fn hypr_lua_dispatch(lua_expr: &str) -> bool {
    match run_hypr_dispatch(&[lua_expr]) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("hyprctl dispatch {} failed: {}", lua_expr, err);
            false
        }
    }
}

pub fn window_is_floating(win: Window) -> Option<bool> {
    let clients = run_hyprctl_json(&["-j", "clients"])?;
    clients
        .as_array()?
        .iter()
        .find(|client| json_to_window_address(&client["address"]) == Some(win))
        .and_then(|client| client["floating"].as_bool())
}

pub fn window_geometry(win: Window) -> Option<(i32, i32, i32, i32)> {
    let clients = run_hyprctl_json(&["-j", "clients"])?;
    let client = clients
        .as_array()?
        .iter()
        .find(|client| json_to_window_address(&client["address"]) == Some(win))?;
    let at = client["at"].as_array()?;
    let size = client["size"].as_array()?;
    Some((
        at.get(0)?.as_i64()? as i32,
        at.get(1)?.as_i64()? as i32,
        size.get(0)?.as_i64()? as i32,
        size.get(1)?.as_i64()? as i32,
    ))
}

pub fn hypr_tile_window(win: Window, x: i32, y: i32, width: i32, height: i32) -> bool {
    let selector = format!("address:0x{:x}", win);
    let selector = lua_string(&selector);
    let lua_expr = format!(
        concat!(
            "function() ",
            "hl.dispatch(hl.dsp.focus({{ window = {selector} }})); ",
            "hl.dispatch(hl.dsp.window.float({{ action = \"set\" }})); ",
            "hl.dispatch(hl.dsp.window.resize({{ x = {width}, y = {height}, window = {selector} }})); ",
            "hl.dispatch(hl.dsp.window.move({{ x = {x}, y = {y}, window = {selector} }})); ",
            "hl.dispatch(hl.dsp.window.alter_zorder({{ mode = \"top\", window = {selector} }})); ",
            "end"
        ),
        selector = selector,
        x = x,
        y = y,
        width = width,
        height = height,
    );

    hypr_lua_dispatch(&lua_expr)
}

pub fn get_wm_data() -> (
    Rc<Vec<(Window, Workspace, String, String)>>,
    Rc<Monitor>,
    Workspace,
    Window,
) {
    let monitors = run_hyprctl_json(&["-j", "monitors"]);
    let geom = monitors
        .as_ref()
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .find(|monitor| monitor["focused"].as_bool().unwrap_or(false))
                .or_else(|| arr.first())
        })
        .map(|monitor| Monitor {
            width: monitor["width"].as_u64().unwrap_or_default() as u32,
            height: monitor["height"].as_u64().unwrap_or_default() as u32,
            scale: monitor["scale"].as_f64().unwrap_or(1.0) as f32,
        })
        .unwrap_or(Monitor {
            width: 0,
            height: 0,
            scale: 1.0,
        });

    let clients = run_hyprctl_json(&["-j", "clients"]).unwrap_or(Value::Array(vec![]));
    // Extract wins (address, workspace.id, title, class)
    let wins = clients
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|client| {
            let address = json_to_window_address(&client["address"])?;
            let workspace_id = json_to_workspace_id(&client["workspace"]["id"])?;
            let title = client["title"].as_str().unwrap_or_default().to_string();
            let class = client["class"].as_str().unwrap_or_default().to_string();
            Some((address, workspace_id, title, class))
        })
        .collect::<Vec<_>>();

    let cur_desktop = run_hyprctl_json(&["-j", "activeworkspace"])
        .and_then(|workspace| json_to_workspace_id(&workspace["id"]))
        .unwrap_or_default();

    let cur_window = run_hyprctl_json(&["-j", "activewindow"])
        .and_then(|window| json_to_window_address(&window["address"]))
        .unwrap_or_default();

    (Rc::new(wins), Rc::new(geom), cur_desktop, cur_window)
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
    wins: &Rc<Vec<(Window, Workspace, String, String)>>,
    desktop: Option<Workspace>,
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
    let address = format!("address:0x{:x}", win);
    hypr_dispatch("focuswindow", &address);
    thread::sleep(Duration::from_millis(100));
    hypr_dispatch("alterzorder", format!("top,{}", address));
}

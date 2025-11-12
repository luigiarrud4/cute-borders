#![windows_subsystem = "windows"]
#![allow(unused_assignments)]

// --- Importações de bibliotecas externas ---
use check_elevation::is_elevated;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItemBuilder};
use tray_icon::{Icon, TrayIconBuilder};
use winapi::ctypes::c_int;
use winapi::ctypes::c_void;
use winapi::shared::minwindef::{BOOL, LPARAM};
use winapi::shared::windef::HWND;
use winapi::um::dwmapi::DwmSetWindowAttribute;
use winapi::um::shellapi::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use winapi::um::winuser::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindowTextLengthW, GetWindowTextW,
    IsWindow, IsWindowVisible, TranslateMessage, DispatchMessageW,
    GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

// --- Importações da biblioteca padrão do Rust ---
use std::ffi::{c_ulong, OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::prelude::OsStringExt;
use std::ptr;
use std::time::Duration;

// --- Nossos Módulos Internos ---
mod config;
mod logger;
mod rainbow;
mod util;
mod gui;

// --- Importações dos nossos módulos ---
use config::{Config, RuleMatch};
use logger::Logger;
use rainbow::Rainbow;
use util::{get_exe_path, hex_to_colorref, set_startup};

const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_COLOR_DEFAULT: u32 = 0xFFFFFFFF;
const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
const COLOR_INVALID: u32 = 0x000000FF;

// --- NOVO HELPER ---
// Função auxiliar para verificar se o modo Rainbow está ativo na configuração global.
fn is_rainbow_active(config: &Config) -> bool {
    config.window_rules.iter().any(|r| {
        r.rule_match == RuleMatch::Global && r.active_border_color.to_lowercase() == "rainbow"
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--config-gui" {
        gui::run_gui();
        return;
    }

    if let Err(err) = set_startup(true) {
        Logger::log(&format!("[ERROR] Failed to create or update startup task: {:?}", err));
    }

    // --- LOOP DE ATUALIZAÇÃO REFORMULADO ---
    std::thread::spawn(|| {
        let mut last_active_window: HWND = ptr::null_mut();

        loop {
            let config = Config::get();
            let rainbow_is_on = is_rainbow_active(&config);

            // 1. O 'tick' do Rainbow é chamado sempre para que a cor continue a transição.
            if rainbow_is_on {
                Rainbow::tick(config.rainbow_speed.unwrap_or(1.0));
            }

            let current_active_window = unsafe { GetForegroundWindow() };

            // 2. As cores só são aplicadas se a janela ativa mudou OU se o rainbow está ligado.
            if current_active_window != last_active_window || rainbow_is_on {
                apply_colors(false);
                last_active_window = current_active_window;
            }
            
            // Dormimos por um período curto para manter a responsividade e o efeito rainbow suave.
            // Este loop agora é muito mais leve.
            std::thread::sleep(Duration::from_millis(33)); // ~30 FPS para o rainbow
        }
    });

    let is_elevated = is_elevated().unwrap_or(false);
    unsafe {
        #[allow(unused_variables)]
        let tray_icon;
        if !Config::get().hide_tray_icon.unwrap_or(false) {
            let tray_menu_builder = Menu::with_items(&[
                &MenuItemBuilder::new().text("Open config").enabled(true).id(MenuId::new("0")).build(),
                &MenuItemBuilder::new().text(if is_elevated { "Uninstall" } else { "Install" }).enabled(true).id(MenuId::new("1")).build(),
                &MenuItemBuilder::new().text("Exit").enabled(true).id(MenuId::new("2")).build(),
            ]);

            let tray_menu = match tray_menu_builder {
                Ok(tray_menu) => tray_menu,
                Err(err) => { Logger::log(&format!("[ERROR] Failed to build tray menu: {:?}", err)); std::process::exit(1); }
            };

            let icon = match Icon::from_resource(1, Some((64, 64))) {
                Ok(icon) => icon,
                Err(err) => { Logger::log(&format!("[ERROR] Failed to create icon: {:?}", err)); std::process::exit(1); }
            };

            let tray_icon_builder = TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_menu_on_left_click(true)
                .with_icon(icon)
                .with_tooltip(format!("cute-borders v{}", env!("CARGO_PKG_VERSION")));

            tray_icon = match tray_icon_builder.build() {
                Ok(tray_icon) => tray_icon,
                Err(err) => { Logger::log(&format!("[ERROR] Failed to build tray icon: {:?}", err)); std::process::exit(1); }
            };

            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if event.id == MenuId::new("0") {
                    if let Ok(current_exe) = std::env::current_exe() {
                        std::process::Command::new(current_exe).arg("--config-gui").spawn().ok();
                    }
                } else if event.id == MenuId::new("1") {
                    if is_elevated {
                        if let Err(err) = set_startup(false) { Logger::log(&format!("[ERROR] Failed to remove startup task: {:?}", err)); }
                        apply_colors(true);
                        std::process::exit(0);
                    } else {
                        let lp_verb: Vec<u16> = OsStr::new("runas").encode_wide().chain(std::iter::once(0)).collect();
                        let d = get_exe_path();
                        let v = d.to_str().unwrap_or_default();
                        let lp_file: Vec<u16> = OsStr::new(&v).encode_wide().chain(std::iter::once(0)).collect();
                        let lp_par: Vec<u16> = OsStr::new("").encode_wide().chain(std::iter::once(0)).collect();
                        let mut sei = SHELLEXECUTEINFOW {
                            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32, fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
                            lpVerb: lp_verb.as_ptr(), lpFile: lp_file.as_ptr(), lpParameters: lp_par.as_ptr(), nShow: 1,
                            dwHotKey: 0, hInstApp: ptr::null_mut(), hMonitor: ptr::null_mut(), hProcess: ptr::null_mut(),
                            hkeyClass: ptr::null_mut(), hwnd: ptr::null_mut(), lpClass: ptr::null_mut(),
                            lpDirectory: ptr::null_mut(), lpIDList: ptr::null_mut(),
                        };
                        ShellExecuteExW(&mut sei);
                        apply_colors(true);
                        std::process::exit(0);
                    }
                } else if event.id == MenuId::new("2") {
                    apply_colors(true);
                    std::process::exit(0);
                }
            }));
        }

        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        apply_colors(true);
    }
}


unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
  if IsWindowVisible(hwnd) != 0 {
    let mut title_buffer: [u16; 512] = [0; 512];
    let text_length = GetWindowTextLengthW(hwnd) + 1;
    if text_length > 0 { GetWindowTextW(hwnd, title_buffer.as_mut_ptr(), text_length as c_int); }
    let title = OsString::from_wide(&title_buffer).to_string_lossy().into_owned();
    let ex_style = winapi::um::winuser::GetWindowLongW(hwnd, GWL_EXSTYLE) as c_int;
    let mut class_buffer: [u16; 256] = [0; 256];
    let class_result = GetClassNameW(hwnd, class_buffer.as_mut_ptr(), class_buffer.len() as c_int);
    let mut class_name = String::new();
    if class_result > 0 { class_name = OsString::from_wide(&class_buffer).to_string_lossy().into_owned(); }
    if ex_style & (WS_EX_TOOLWINDOW as i32) == 0 {
      let visible_windows: &mut Vec<(HWND, String, String)> = &mut *(lparam as *mut Vec<(HWND, String, String)>);
      visible_windows.push((hwnd, title, class_name));
    }
  }
  1
}

fn get_colors_for_window(_hwnd: HWND, title: String, class: String, reset: bool) -> (u32, u32) {
  if reset { return (DWMWA_COLOR_DEFAULT, DWMWA_COLOR_DEFAULT); }
  let config = Config::get();
  let mut color_active = COLOR_INVALID;
  let mut color_inactive = COLOR_INVALID;

  for rule in config.window_rules.iter() {
    let rule_applies = match rule.rule_match {
        RuleMatch::Global => true,
        RuleMatch::Title => {
            if let Some(contains_str) = &rule.contains {
                title.to_lowercase().contains(&contains_str.to_lowercase())
            } else {
                Logger::log("Expected `contains` on `Match=\"Title\"`");
                false
            }
        }
        RuleMatch::Class => {
            if let Some(contains_str) = &rule.contains {
                class.to_lowercase().contains(&contains_str.to_lowercase())
            } else {
                Logger::log("Expected `contains` on `Match=\"Class\"`");
                false
            }
        }
    };

    if rule_applies {
        color_active = hex_to_colorref(&rule.active_border_color);
        color_inactive = if rule.inactive_border_color.is_empty() {
            DWMWA_COLOR_DEFAULT
        } else {
            hex_to_colorref(&rule.inactive_border_color)
        };
        
        if rule.rule_match != RuleMatch::Global {
            break; // Aplica a primeira regra específica que encontrar
        }
    }
  }
  (color_active, color_inactive)
}

fn apply_colors(reset: bool) {
  let mut visible_windows: Vec<(HWND, String, String)> = Vec::new();
  unsafe { EnumWindows(Some(enum_windows_callback), &mut visible_windows as *mut _ as LPARAM); }
  let active_hwnd = unsafe { GetForegroundWindow() };

  for (hwnd, title, class) in visible_windows {
    if unsafe { IsWindow(hwnd) } == 0 { continue; }
    let (color_active, color_inactive) = get_colors_for_window(hwnd, title, class, reset);
    
    let color_to_apply = if active_hwnd == hwnd { color_active } else { color_inactive };

    if color_to_apply != COLOR_INVALID {
        unsafe {
            DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &color_to_apply as *const _ as *const c_void, std::mem::size_of::<c_ulong>() as u32);
        }
    }
  }
}
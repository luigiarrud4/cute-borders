// src/main.rs

#![windows_subsystem = "windows"]
#![allow(unused_assignments)]

// --- Importações ---
use check_elevation::is_elevated;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItemBuilder};
use tray_icon::{Icon, TrayIconBuilder};
use winapi::ctypes::{c_int, c_void};
use winapi::shared::minwindef::{BOOL, DWORD, LPARAM};
use winapi::shared::windef::{HWND, HWINEVENTHOOK};
use winapi::um::dwmapi::DwmSetWindowAttribute;
use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
use winapi::um::shellapi::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use winapi::um::winnt::PROCESS_TERMINATE;
use winapi::um::winuser::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostQuitMessage,
    TranslateMessage, DispatchMessageW, GWL_EXSTYLE, WS_EX_TOOLWINDOW, SetWinEventHook, UnhookWinEvent,
    EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, GetWindow, GW_OWNER,
};
use std::ffi::{c_ulong, OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::prelude::OsStringExt;
use std::ptr;
use std::sync::Mutex;
use std::time::Duration;
use std::mem;

// --- Módulos Internos ---
mod config;
mod logger;
mod rainbow;
mod util;
mod gui;

// --- Importações dos Módulos ---
use config::{Config, RuleMatch};
use logger::Logger;
use rainbow::Rainbow;
use util::{get_exe_path, hex_to_colorref, set_startup};

// --- Constantes e Globais ---
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_COLOR_DEFAULT: u32 = 0xFFFFFFFF;
#[allow(dead_code)]
const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
const COLOR_INVALID: u32 = 0x000000FF;

static GUI_PID: Mutex<Option<DWORD>> = Mutex::new(None);

// --- Lógica Principal ---

fn is_rainbow_active(config: &Config) -> bool {
    config.window_rules.iter().any(|r| {
        r.rule_match == RuleMatch::Global && r.active_border_color.to_lowercase() == "rainbow"
    })
}

unsafe extern "system" fn win_event_proc(
    _h_win_event_hook: HWINEVENTHOOK, event: u32, hwnd: HWND,
    _id_object: i32, _id_child: i32, _id_event_thread: u32, _dwms_event_time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        // O Hook agora só precisa se preocupar em repintar se o rainbow estiver DESLIGADO.
        if !is_rainbow_active(&Config::get()) {
            apply_colors(hwnd, false);
        }
    }
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 { return 1; }
    let mut class_buffer: [u16; 256] = [0; 256];
    GetClassNameW(hwnd, class_buffer.as_mut_ptr(), class_buffer.len() as c_int);
    let class_name = OsString::from_wide(&class_buffer).to_string_lossy().into_owned();
    let ex_style = winapi::um::winuser::GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

    if (ex_style & WS_EX_TOOLWINDOW == 0) || class_name.contains("#32768") {
        let mut title_buffer: [u16; 512] = [0; 512];
        if GetWindowTextLengthW(hwnd) > 0 {
            GetWindowTextW(hwnd, title_buffer.as_mut_ptr(), title_buffer.len() as c_int);
        }
        let title = OsString::from_wide(&title_buffer).to_string_lossy().into_owned();
        let visible_windows: &mut Vec<(HWND, String, String)> = &mut *(lparam as *mut Vec<(HWND, String, String)>);
        visible_windows.push((hwnd, title, class_name));
    }
    1
}

unsafe fn is_part_of_active_chain(hwnd: HWND, active_hwnd: HWND) -> bool {
    if active_hwnd.is_null() || hwnd.is_null() { return false; }
    if hwnd == active_hwnd { return true; }
    let owner = GetWindow(hwnd, GW_OWNER);
    if owner.is_null() { return false; }
    is_part_of_active_chain(owner, active_hwnd)
}

fn get_window_pid(hwnd: HWND) -> u32 {
    let mut pid: DWORD = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

fn apply_colors(active_hwnd: HWND, reset: bool) {
    let mut visible_windows: Vec<(HWND, String, String)> = Vec::new();
    let lparam = &mut visible_windows as *mut _ as LPARAM;
    unsafe { EnumWindows(Some(enum_windows_callback), lparam) };
    let active_pid = if !active_hwnd.is_null() { get_window_pid(active_hwnd) } else { 0 };

    for (hwnd, title, class) in visible_windows {
        if unsafe { IsWindow(hwnd) } == 0 { continue; }
        let (color_active, color_inactive) = get_colors_for_window(hwnd, title, class.clone(), reset);
        let is_in_owner_chain = unsafe { is_part_of_active_chain(hwnd, active_hwnd) };
        let window_pid = get_window_pid(hwnd);
        let is_special_menu_of_active_process = active_pid != 0 && window_pid == active_pid && class.contains("#32768");
        let is_considered_active = is_in_owner_chain || is_special_menu_of_active_process;
        let color_to_apply = if is_considered_active { color_active } else { color_inactive };

        if color_to_apply != COLOR_INVALID {
            unsafe {
                let _ = DwmSetWindowAttribute(
                    hwnd, DWMWA_BORDER_COLOR,
                    &color_to_apply as *const _ as *const c_void,
                    mem::size_of::<c_ulong>() as u32,
                );
            }
        }
    }
}

fn get_colors_for_window(_hwnd: HWND, title: String, class: String, reset: bool) -> (u32, u32) {
    if reset { return (DWMWA_COLOR_DEFAULT, DWMWA_COLOR_DEFAULT); }
    let config = Config::get();
    let mut color_active = COLOR_INVALID;
    let mut color_inactive = COLOR_INVALID;

    for rule in config.window_rules.iter() {
        let rule_applies = match rule.rule_match {
            RuleMatch::Global => true,
            RuleMatch::Title => rule.contains.as_ref().map_or(false, |c| title.to_lowercase().contains(&c.to_lowercase())),
            RuleMatch::Class => rule.contains.as_ref().map_or(false, |c| class.to_lowercase().contains(&c.to_lowercase())),
        };

        if rule_applies {
            color_active = hex_to_colorref(&rule.active_border_color);
            color_inactive = if rule.inactive_border_color.is_empty() { DWMWA_COLOR_DEFAULT } else { hex_to_colorref(&rule.inactive_border_color) };
            if rule.rule_match != RuleMatch::Global { break; }
        }
    }
    (color_active, color_inactive)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--config-gui" { gui::run_gui(); return; }
    if let Err(err) = set_startup(true) { Logger::log(&format!("[ERROR] Falha ao criar tarefa: {:?}", err)); }

    // THREAD 1: O Mestre da Pintura
    std::thread::spawn(|| {
        loop {
            let config = Config::get();
            let active_hwnd = unsafe { GetForegroundWindow() };

            if is_rainbow_active(&config) {
                Rainbow::tick(config.rainbow_speed.unwrap_or(1.0));
            }

            // [A MUDANÇA CRÍTICA]: A função apply_colors é chamada a cada quadro,
            // garantindo que qualquer mudança no config.yaml (seja para rainbow
            // ou cor estática) seja aplicada imediatamente.
            apply_colors(active_hwnd, false);
            
            std::thread::sleep(Duration::from_millis(33));
        }
    });

    // THREAD 2: Ouvinte de Eventos do Windows (para resposta instantânea em modo estático)
    std::thread::spawn(|| {
        unsafe {
            let hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, ptr::null_mut(), Some(win_event_proc), 0, 0, WINEVENT_OUTOFCONTEXT);
            let mut msg = mem::zeroed();
            while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if !hook.is_null() { UnhookWinEvent(hook); }
        }
    });

    // THREAD PRINCIPAL: Cuida do Ícone da Bandeja
    let is_elevated = is_elevated().unwrap_or(false);
    unsafe {
        let mut tray_icon_instance = None;
        if !Config::get().hide_tray_icon.unwrap_or(false) {
            let tray_menu = Menu::with_items(&[
                &MenuItemBuilder::new().text("Abrir Configurações").id(MenuId::new("open_gui")).enabled(true).build(),
                &MenuItemBuilder::new().text(if is_elevated { "Desinstalar" } else { "Instalar (Requer Admin)" }).id(MenuId::new("install")).enabled(true).build(),
                &MenuItemBuilder::new().text("Sair").id(MenuId::new("quit")).enabled(true).build(),
            ]).expect("Falha ao criar menu");

            let icon = Icon::from_resource(1, Some((64, 64))).expect("Falha ao carregar ícone");
            tray_icon_instance = Some(TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_menu_on_left_click(true)
                .with_icon(icon)
                .with_tooltip(format!("cute-borders v{}", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("Falha ao criar ícone da bandeja"));
        }

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            match event.id.0.as_str() {
                "open_gui" => {
                    if let Ok(cmd) = std::process::Command::new(std::env::current_exe().unwrap()).arg("--config-gui").spawn() {
                        *GUI_PID.lock().unwrap() = Some(cmd.id());
                    }
                },
                "install" => {
                    if is_elevated {
                        if let Err(err) = set_startup(false) { Logger::log(&format!("[ERROR] Falha ao remover tarefa: {:?}", err)); }
                        shutdown_app();
                    } else {
                        let exe_path = get_exe_path();
                        let lp_verb: Vec<u16> = OsStr::new("runas").encode_wide().chain(std::iter::once(0)).collect();
                        let lp_file: Vec<u16> = OsStr::new(exe_path.to_str().unwrap_or_default()).encode_wide().chain(std::iter::once(0)).collect();
                        let mut sei = SHELLEXECUTEINFOW { cbSize: mem::size_of::<SHELLEXECUTEINFOW>() as u32, fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS, lpVerb: lp_verb.as_ptr(), lpFile: lp_file.as_ptr(), ..mem::zeroed() };
                        ShellExecuteExW(&mut sei);
                        shutdown_app();
                    }
                },
                "quit" => shutdown_app(),
                _ => {}
            }
        }));

        let mut msg = mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(tray_icon_instance);
    }
}

fn shutdown_app() {
    apply_colors(ptr::null_mut(), true);
    if let Some(pid) = *GUI_PID.lock().unwrap() {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !handle.is_null() { TerminateProcess(handle, 1); }
        }
    }
    unsafe { PostQuitMessage(0) };
}

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use sysinfo::{CpuExt, PidExt, ProcessExt, System, SystemExt};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const APP_NAME: &str = "Resource Monitor";

#[derive(Serialize, Clone)]
struct ProcessInfo {
    name: String,
    pid: u32,
    cpu_percent: f32,
    gpu_percent: f32,
    memory_percent: f32,
}

#[derive(Serialize)]
struct SystemInfo {
    cpu_percent: f32,
    gpu_percent: f32,
    memory_percent: f32,
    top_processes: Vec<ProcessInfo>,
    top_gpu_processes: Vec<ProcessInfo>,
    top_memory_processes: Vec<ProcessInfo>,
}

struct AppState {
    system: Mutex<System>,
    gpu: Mutex<GpuSampler>,
    opacity_percent: Mutex<u8>,
}

impl AppState {
    fn new() -> Self {
        Self {
            system: Mutex::new(System::new()),
            gpu: Mutex::new(GpuSampler::new()),
            opacity_percent: Mutex::new(92),
        }
    }

    fn refresh(&self) {
        if let Ok(mut system) = self.system.lock() {
            system.refresh_cpu();
            system.refresh_memory();
            system.refresh_processes();
        }
    }
}

#[tauri::command]
fn hide_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn get_window_opacity(state: State<AppState>) -> Result<u8, String> {
    state
        .opacity_percent
        .lock()
        .map(|opacity| *opacity)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn apply_window_opacity(
    app: AppHandle,
    state: State<AppState>,
    opacity_percent: u8,
) -> Result<u8, String> {
    let opacity_percent = opacity_percent.clamp(40, 100);

    {
        let mut opacity = state.opacity_percent.lock().map_err(|e| e.to_string())?;
        *opacity = opacity_percent;
    }

    let _ = app.emit_to("main", "opacity-changed", opacity_percent);

    if let Some(window) = app.get_webview_window("main") {
        let opacity = opacity_percent as f32 / 100.0;
        let script = format!(
            "document.documentElement.style.setProperty('--panel-opacity', '{opacity:.2}')"
        );
        let _ = window.eval(script);
    }

    Ok(opacity_percent)
}

#[tauri::command]
fn get_system_info(state: State<AppState>) -> Result<SystemInfo, String> {
    state.refresh();

    let gpu_sample = state
        .gpu
        .lock()
        .map(|mut gpu| gpu.sample())
        .unwrap_or_default();

    let system = state.system.lock().map_err(|e| e.to_string())?;
    let cpu_percent = system.global_cpu_info().cpu_usage();
    let cpu_count = system.cpus().len().max(1) as f32;
    let total_memory = system.total_memory().max(1) as f32;
    let memory_percent = ((system.used_memory() as f32 / total_memory) * 100.0).clamp(0.0, 100.0);

    let mut processes: Vec<ProcessInfo> = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let process_cpu_percent = (process.cpu_usage() / cpu_count).clamp(0.0, 100.0);
            let process_gpu_percent = gpu_sample
                .by_pid
                .get(&pid.as_u32())
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, gpu_sample.total);
            let process_memory_percent =
                ((process.memory() as f32 / total_memory) * 100.0).clamp(0.0, 100.0);

            ProcessInfo {
                name: process.name().to_string(),
                pid: pid.as_u32(),
                cpu_percent: process_cpu_percent,
                gpu_percent: process_gpu_percent,
                memory_percent: process_memory_percent,
            }
        })
        .collect();

    processes.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_processes = processes.iter().take(5).cloned().collect();

    let mut gpu_processes = processes.clone();
    gpu_processes.sort_by(|a, b| {
        b.gpu_percent
            .partial_cmp(&a.gpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let top_gpu_processes = gpu_processes.into_iter().take(5).collect();

    let mut memory_processes = processes;
    memory_processes.sort_by(|a, b| {
        b.memory_percent
            .partial_cmp(&a.memory_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let top_memory_processes = memory_processes.into_iter().take(5).collect();

    Ok(SystemInfo {
        cpu_percent,
        gpu_percent: gpu_sample.total,
        memory_percent,
        top_processes,
        top_gpu_processes,
        top_memory_processes,
    })
}

fn show_main(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

fn show_settings(app: &AppHandle) -> Result<(), String> {
    const SETTINGS_WIDTH: f64 = 328.0;
    const SETTINGS_HEIGHT: f64 = 228.0;

    let window = if let Some(window) = app.get_webview_window("settings") {
        window
    } else {
        WebviewWindowBuilder::new(
            app,
            "settings",
            WebviewUrl::App("index.html?mode=settings".into()),
        )
        .title("Resource Monitor Settings")
        .inner_size(SETTINGS_WIDTH, SETTINGS_HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(false)
        .center()
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?
    };

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let title_item = MenuItem::with_id(app, "app_title", APP_NAME, false, None::<&str>)?;
    let separator_top = PredefinedMenuItem::separator(app)?;
    let show_main_item = MenuItem::with_id(app, "show_main", "显示主窗口", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let separator_settings = PredefinedMenuItem::separator(app)?;
    let auto_start_item =
        CheckMenuItem::with_id(app, "auto_start", "开机自启动", true, false, None::<&str>)?;
    let notifications_item =
        CheckMenuItem::with_id(app, "notifications", "通知提醒", true, true, None::<&str>)?;
    let separator_quit = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &title_item,
            &separator_top,
            &show_main_item,
            &settings_item,
            &separator_settings,
            &auto_start_item,
            &notifications_item,
            &separator_quit,
            &quit_item,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("resource-monitor")
        .menu(&menu)
        .tooltip(format!("{APP_NAME} - 正在后台运行"))
        .show_menu_on_left_click(false)
        .icon(fallback_tray_icon())
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show_main" => {
                let _ = show_main(app);
            }
            "settings" => {
                let _ = show_settings(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    app.manage(tray);

    Ok(())
}

fn fallback_tray_icon() -> Image<'static> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let distance = (dx * dx + dy * dy).sqrt();
            let inside = distance <= 14.0;
            let pulse = (x > 7 && x < 25 && (y == 14 || y == 15 || y == 16))
                || (x == 11 && y > 9 && y < 20)
                || (x == 20 && y > 8 && y < 22);

            if pulse {
                rgba.extend_from_slice(&[125, 211, 252, 255]);
            } else if inside {
                rgba.extend_from_slice(&[13, 17, 23, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    Image::new_owned(rgba, SIZE, SIZE)
}

#[derive(Default)]
struct GpuSample {
    total: f32,
    by_pid: HashMap<u32, f32>,
}

#[cfg(target_os = "windows")]
struct GpuSampler {
    query: isize,
    counter: isize,
    available: bool,
}

#[cfg(target_os = "windows")]
impl GpuSampler {
    fn new() -> Self {
        let mut sampler = Self {
            query: 0,
            counter: 0,
            available: false,
        };

        unsafe {
            if pdh::PdhOpenQueryW(std::ptr::null(), 0, &mut sampler.query) != pdh::ERROR_SUCCESS {
                return sampler;
            }

            let path = wide(r"\GPU Engine(*)\Utilization Percentage");
            if pdh::PdhAddEnglishCounterW(sampler.query, path.as_ptr(), 0, &mut sampler.counter)
                != pdh::ERROR_SUCCESS
            {
                pdh::PdhCloseQuery(sampler.query);
                sampler.query = 0;
                return sampler;
            }

            sampler.available = pdh::PdhCollectQueryData(sampler.query) == pdh::ERROR_SUCCESS;
        }

        sampler
    }

    fn sample(&mut self) -> GpuSample {
        if !self.available {
            return GpuSample::default();
        }

        unsafe {
            if pdh::PdhCollectQueryData(self.query) != pdh::ERROR_SUCCESS {
                return GpuSample::default();
            }

            let mut buffer_size = 0;
            let mut item_count = 0;
            let status = pdh::PdhGetFormattedCounterArrayW(
                self.counter,
                pdh::PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                std::ptr::null_mut(),
            );

            if status as u32 != pdh::PDH_MORE_DATA || item_count == 0 {
                return GpuSample::default();
            }

            let item_size = std::mem::size_of::<pdh::PdhFmtCountervalueItemW>();
            let capacity = (buffer_size as usize + item_size - 1) / item_size;
            let mut items =
                Vec::<std::mem::MaybeUninit<pdh::PdhFmtCountervalueItemW>>::with_capacity(capacity);

            let status = pdh::PdhGetFormattedCounterArrayW(
                self.counter,
                pdh::PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                items.as_mut_ptr() as *mut pdh::PdhFmtCountervalueItemW,
            );

            if status != pdh::ERROR_SUCCESS {
                return GpuSample::default();
            }

            items.set_len(item_count as usize);
            let mut by_pid = HashMap::<u32, f32>::new();
            let mut total = 0.0;

            for item in &items {
                let item = item.assume_init_ref();
                let value = item.fmt_value.value.double_value;
                if !value.is_finite() || value <= 0.0 {
                    continue;
                }

                let percent = value as f32;
                total += percent;

                if let Some(pid) = parse_gpu_pid(item.name) {
                    let entry = by_pid.entry(pid).or_insert(0.0);
                    *entry = (*entry + percent).clamp(0.0, 100.0);
                }
            }

            GpuSample {
                total: total.clamp(0.0, 100.0),
                by_pid,
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for GpuSampler {
    fn drop(&mut self) {
        if self.query != 0 {
            unsafe {
                pdh::PdhCloseQuery(self.query);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct GpuSampler;

#[cfg(not(target_os = "windows"))]
impl GpuSampler {
    fn new() -> Self {
        Self
    }

    fn sample(&mut self) -> GpuSample {
        GpuSample::default()
    }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn parse_gpu_pid(name: *mut u16) -> Option<u32> {
    let name = wide_ptr_to_string(name)?;
    let start = name.find("pid_")? + 4;
    let digits = name[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

#[cfg(target_os = "windows")]
fn wide_ptr_to_string(value: *mut u16) -> Option<String> {
    if value.is_null() {
        return None;
    }

    unsafe {
        let mut len = 0;
        while *value.add(len) != 0 {
            len += 1;
        }

        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            value, len,
        )))
    }
}

#[cfg(target_os = "windows")]
mod pdh {
    pub const ERROR_SUCCESS: i32 = 0;
    pub const PDH_FMT_DOUBLE: u32 = 0x0000_0200;
    pub const PDH_MORE_DATA: u32 = 0x8000_07D2;

    #[repr(C)]
    pub union PdhFmtValue {
        pub long_value: i32,
        pub double_value: f64,
        pub large_value: i64,
        pub wide_string_value: *const u16,
    }

    #[repr(C)]
    pub struct PdhFmtCountervalue {
        pub c_status: u32,
        pub value: PdhFmtValue,
    }

    #[repr(C)]
    pub struct PdhFmtCountervalueItemW {
        pub name: *mut u16,
        pub fmt_value: PdhFmtCountervalue,
    }

    #[link(name = "pdh")]
    extern "system" {
        pub fn PdhOpenQueryW(data_source: *const u16, user_data: usize, query: *mut isize) -> i32;
        pub fn PdhAddEnglishCounterW(
            query: isize,
            counter_path: *const u16,
            user_data: usize,
            counter: *mut isize,
        ) -> i32;
        pub fn PdhCollectQueryData(query: isize) -> i32;
        pub fn PdhGetFormattedCounterArrayW(
            counter: isize,
            format: u32,
            buffer_size: *mut u32,
            item_count: *mut u32,
            item_buffer: *mut PdhFmtCountervalueItemW,
        ) -> i32;
        pub fn PdhCloseQuery(query: isize) -> i32;
    }
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::CloseRequested { api, .. } if window.label() == "settings" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            hide_main_window,
            get_window_opacity,
            apply_window_opacity
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use sysinfo::{CpuExt, PidExt, ProcessExt, System, SystemExt};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const APP_NAME: &str = "Resource Monitor";
const APPEARANCE_CONFIG_FILE: &str = "appearance.json";

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
    cpu: Mutex<CpuSampler>,
    gpu: Mutex<GpuSampler>,
    opacity_percent: Mutex<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AppearanceConfig {
    opacity: u8,
    border_radius: u8,
    theme_mode: String,
    accent_color: String,
    font_size: String,
    background_blur: bool,
    animations: bool,
    window_shadow: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AppearanceConfigPatch {
    opacity: Option<u8>,
    border_radius: Option<u8>,
    theme_mode: Option<String>,
    accent_color: Option<String>,
    font_size: Option<String>,
    background_blur: Option<bool>,
    animations: Option<bool>,
    window_shadow: Option<bool>,
}

impl AppState {
    fn new() -> Self {
        Self {
            system: Mutex::new(System::new()),
            cpu: Mutex::new(CpuSampler::new()),
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
fn get_appearance_config(app: AppHandle) -> Result<AppearanceConfig, String> {
    read_appearance_config(&app)
}

#[tauri::command]
fn save_appearance_config(
    app: AppHandle,
    config: AppearanceConfig,
) -> Result<AppearanceConfig, String> {
    let config = config.sanitized();
    write_appearance_config(&app, &config)?;
    let _ = app.emit("appearance-changed", config.clone());
    Ok(config)
}

#[tauri::command]
fn reset_appearance_config(app: AppHandle) -> Result<AppearanceConfig, String> {
    let config = default_appearance_config();
    write_appearance_config(&app, &config)?;
    let _ = app.emit("appearance-changed", config.clone());
    Ok(config)
}

#[tauri::command]
fn terminate_process(pid: u32) -> Result<(), String> {
    validate_process_action_pid(pid)?;

    let mut system = System::new_all();
    system.refresh_processes();
    let process = system
        .process(sysinfo::Pid::from_u32(pid))
        .ok_or_else(|| format!("进程 {pid} 已不存在"))?;
    let name = process.name().to_string();

    if process.kill() {
        Ok(())
    } else {
        Err(format!("无法结束进程 {name} ({pid})，可能需要管理员权限"))
    }
}

#[tauri::command]
fn open_process_location(pid: u32) -> Result<(), String> {
    validate_pid(pid)?;

    let mut system = System::new_all();
    system.refresh_processes();
    let process = system
        .process(sysinfo::Pid::from_u32(pid))
        .ok_or_else(|| format!("进程 {pid} 已不存在"))?;
    let exe = process.exe();

    if exe.as_os_str().is_empty() {
        return Err(format!("进程 {pid} 未提供可执行文件路径"));
    }

    Command::new("explorer")
        .arg(explorer_select_arg(exe))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开文件所在位置: {e}"))
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
    let fallback_cpu_percent = system.global_cpu_info().cpu_usage();
    let cpu_percent = state
        .cpu
        .lock()
        .ok()
        .and_then(|mut cpu| cpu.sample())
        .unwrap_or(fallback_cpu_percent);
    let cpu_count = system.cpus().len().max(1) as f32;
    let total_memory = system.total_memory();
    let memory_percent = memory_usage_percent(total_memory, system.available_memory());

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
            let process_memory_percent = memory_share_percent(process.memory(), total_memory);

            ProcessInfo {
                name: process.name().to_string(),
                pid: pid.as_u32(),
                cpu_percent: process_cpu_percent,
                gpu_percent: process_gpu_percent,
                memory_percent: process_memory_percent,
            }
        })
        .collect();

    processes.sort_by(compare_by_resource_score);
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
        .min_inner_size(SETTINGS_WIDTH, SETTINGS_HEIGHT)
        .resizable(true)
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

fn percent(part: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }

    ((part as f32 / total as f32) * 100.0).clamp(0.0, 100.0)
}

fn memory_usage_percent(total_memory: u64, available_memory: u64) -> f32 {
    percent(total_memory.saturating_sub(available_memory), total_memory)
}

fn memory_share_percent(process_memory: u64, total_memory: u64) -> f32 {
    percent(process_memory, total_memory)
}

fn default_appearance_config() -> AppearanceConfig {
    AppearanceConfig {
        opacity: 92,
        border_radius: 12,
        theme_mode: "system".to_string(),
        accent_color: "#7dd3fc".to_string(),
        font_size: "medium".to_string(),
        background_blur: true,
        animations: true,
        window_shadow: false,
    }
}

impl AppearanceConfig {
    fn sanitized(self) -> Self {
        let defaults = default_appearance_config();

        Self {
            opacity: self.opacity.clamp(40, 100),
            border_radius: self.border_radius.clamp(0, 32),
            theme_mode: sanitize_choice(&self.theme_mode, &["system", "light", "dark"])
                .unwrap_or(defaults.theme_mode),
            accent_color: if is_valid_hex_color(&self.accent_color) {
                self.accent_color
            } else {
                defaults.accent_color
            },
            font_size: sanitize_choice(&self.font_size, &["small", "medium", "large"])
                .unwrap_or(defaults.font_size),
            background_blur: self.background_blur,
            animations: self.animations,
            window_shadow: self.window_shadow,
        }
    }
}

impl AppearanceConfigPatch {
    fn into_config(self) -> AppearanceConfig {
        let defaults = default_appearance_config();

        AppearanceConfig {
            opacity: self.opacity.unwrap_or(defaults.opacity),
            border_radius: self.border_radius.unwrap_or(defaults.border_radius),
            theme_mode: self.theme_mode.unwrap_or(defaults.theme_mode),
            accent_color: self.accent_color.unwrap_or(defaults.accent_color),
            font_size: self.font_size.unwrap_or(defaults.font_size),
            background_blur: self.background_blur.unwrap_or(defaults.background_blur),
            animations: self.animations.unwrap_or(defaults.animations),
            window_shadow: self.window_shadow.unwrap_or(defaults.window_shadow),
        }
        .sanitized()
    }
}

fn appearance_config_from_json(content: &str) -> AppearanceConfig {
    serde_json::from_str::<AppearanceConfigPatch>(content)
        .map(|config| config.into_config())
        .unwrap_or_else(|_| default_appearance_config())
}

fn sanitize_choice(value: &str, allowed: &[&str]) -> Option<String> {
    allowed
        .iter()
        .find(|allowed_value| **allowed_value == value)
        .map(|value| (*value).to_string())
}

fn is_valid_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.chars().skip(1).all(|ch| ch.is_ascii_hexdigit())
}

fn appearance_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(APPEARANCE_CONFIG_FILE))
        .map_err(|e| e.to_string())
}

fn read_appearance_config(app: &AppHandle) -> Result<AppearanceConfig, String> {
    let path = appearance_config_path(app)?;

    if !path.exists() {
        return Ok(default_appearance_config());
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(appearance_config_from_json(&content))
}

fn write_appearance_config(app: &AppHandle, config: &AppearanceConfig) -> Result<(), String> {
    let path = appearance_config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn resource_score(cpu_percent: f32, memory_percent: f32, gpu_percent: f32) -> f32 {
    let cpu_percent = cpu_percent.clamp(0.0, 100.0);
    let memory_percent = memory_percent.clamp(0.0, 100.0);
    let gpu_percent = gpu_percent.clamp(0.0, 100.0);
    let weighted_score = cpu_percent * 0.45 + memory_percent * 0.25 + gpu_percent * 0.3;
    let bottleneck_score = cpu_percent.max(memory_percent).max(gpu_percent);

    weighted_score * 0.6 + bottleneck_score * 0.4
}

fn compare_by_resource_score(a: &ProcessInfo, b: &ProcessInfo) -> std::cmp::Ordering {
    resource_score(b.cpu_percent, b.memory_percent, b.gpu_percent)
        .partial_cmp(&resource_score(
            a.cpu_percent,
            a.memory_percent,
            a.gpu_percent,
        ))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn validate_pid(pid: u32) -> Result<(), String> {
    if pid == 0 {
        Err("无效的进程 ID".to_string())
    } else {
        Ok(())
    }
}

fn validate_process_action_pid(pid: u32) -> Result<(), String> {
    validate_pid(pid)?;

    if pid == std::process::id() {
        Err("不能结束 Resource Monitor 自身进程".to_string())
    } else {
        Ok(())
    }
}

fn explorer_select_arg(path: &Path) -> String {
    format!("/select,{}", path.display())
}

fn record_gpu_engine_usage(
    by_pid: &mut HashMap<u32, f32>,
    by_engine: &mut HashMap<String, f32>,
    pid: Option<u32>,
    engine_key: Option<String>,
    percent: f32,
) {
    let percent = percent.clamp(0.0, 100.0);

    if let Some(pid) = pid {
        let entry = by_pid.entry(pid).or_insert(0.0);
        *entry = (*entry).max(percent);
    }

    if let Some(engine_key) = engine_key {
        let entry = by_engine.entry(engine_key).or_insert(0.0);
        *entry = (*entry + percent).clamp(0.0, 100.0);
    }
}

fn gpu_total_from_engines(by_engine: &HashMap<String, f32>) -> f32 {
    by_engine
        .values()
        .copied()
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 100.0)
}

#[cfg(target_os = "windows")]
struct CpuSampler {
    query: isize,
    counter: isize,
    available: bool,
}

#[cfg(target_os = "windows")]
impl CpuSampler {
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

            let paths = [
                r"\Processor Information(_Total)\% Processor Utility",
                r"\Processor(_Total)\% Processor Time",
            ];

            for path in paths {
                let path = wide(path);
                if pdh::PdhAddEnglishCounterW(sampler.query, path.as_ptr(), 0, &mut sampler.counter)
                    == pdh::ERROR_SUCCESS
                {
                    sampler.available =
                        pdh::PdhCollectQueryData(sampler.query) == pdh::ERROR_SUCCESS;
                    break;
                }
            }

            if !sampler.available {
                pdh::PdhCloseQuery(sampler.query);
                sampler.query = 0;
                sampler.counter = 0;
            }
        }

        sampler
    }

    fn sample(&mut self) -> Option<f32> {
        if !self.available {
            return None;
        }

        unsafe {
            if pdh::PdhCollectQueryData(self.query) != pdh::ERROR_SUCCESS {
                return None;
            }

            let mut value = pdh::PdhFmtCountervalue {
                c_status: 0,
                value: pdh::PdhFmtValue { double_value: 0.0 },
            };

            let status = pdh::PdhGetFormattedCounterValue(
                self.counter,
                pdh::PDH_FMT_DOUBLE,
                std::ptr::null_mut(),
                &mut value,
            );

            if status != pdh::ERROR_SUCCESS || value.c_status != pdh::ERROR_SUCCESS as u32 {
                return None;
            }

            let value = value.value.double_value;
            if value.is_finite() {
                Some((value as f32).clamp(0.0, 100.0))
            } else {
                None
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for CpuSampler {
    fn drop(&mut self) {
        if self.query != 0 {
            unsafe {
                pdh::PdhCloseQuery(self.query);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct CpuSampler;

#[cfg(not(target_os = "windows"))]
impl CpuSampler {
    fn new() -> Self {
        Self
    }

    fn sample(&mut self) -> Option<f32> {
        None
    }
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
            let mut by_engine = HashMap::<String, f32>::new();

            for item in &items {
                let item = item.assume_init_ref();
                let value = item.fmt_value.value.double_value;
                if !value.is_finite() || value <= 0.0 {
                    continue;
                }

                record_gpu_engine_usage(
                    &mut by_pid,
                    &mut by_engine,
                    parse_gpu_pid(item.name),
                    parse_gpu_engine_key(item.name),
                    value as f32,
                );
            }

            GpuSample {
                total: gpu_total_from_engines(&by_engine),
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
fn parse_gpu_engine_key(name: *mut u16) -> Option<String> {
    gpu_engine_key_from_instance_name(&wide_ptr_to_string(name)?)
}

fn gpu_engine_key_from_instance_name(name: &str) -> Option<String> {
    let start = name.find("pid_")? + 4;
    let engine_start = name[start..].find('_')? + start + 1;
    let key = name[engine_start..].trim();

    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
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
        pub fn PdhGetFormattedCounterValue(
            counter: isize,
            format: u32,
            type_: *mut u32,
            value: *mut PdhFmtCountervalue,
        ) -> i32;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_usage_uses_available_memory() {
        assert_eq!(memory_usage_percent(16_000, 4_000), 75.0);
    }

    #[test]
    fn memory_usage_handles_zero_total() {
        assert_eq!(memory_usage_percent(0, 0), 0.0);
        assert_eq!(memory_share_percent(512, 0), 0.0);
    }

    #[test]
    fn memory_usage_saturates_when_available_exceeds_total() {
        assert_eq!(memory_usage_percent(8_000, 9_000), 0.0);
    }

    #[test]
    fn gpu_engine_usage_sums_processes_per_engine_like_task_manager() {
        let mut by_pid = HashMap::new();
        let mut by_engine = HashMap::new();

        record_gpu_engine_usage(
            &mut by_pid,
            &mut by_engine,
            Some(42),
            Some("luid_a_phys_0_eng_0_engtype_3D".to_string()),
            35.0,
        );
        record_gpu_engine_usage(
            &mut by_pid,
            &mut by_engine,
            Some(7),
            Some("luid_a_phys_0_eng_0_engtype_3D".to_string()),
            18.0,
        );
        record_gpu_engine_usage(
            &mut by_pid,
            &mut by_engine,
            Some(9),
            Some("luid_a_phys_0_eng_1_engtype_Copy".to_string()),
            64.0,
        );

        assert_eq!(gpu_total_from_engines(&by_engine), 64.0);
        assert_eq!(by_pid.get(&42), Some(&35.0));
        assert_eq!(by_pid.get(&7), Some(&18.0));
    }

    #[test]
    fn gpu_engine_usage_clamps_engine_sums_to_one_hundred() {
        let mut by_pid = HashMap::new();
        let mut by_engine = HashMap::new();

        record_gpu_engine_usage(
            &mut by_pid,
            &mut by_engine,
            Some(42),
            Some("luid_a_phys_0_eng_0_engtype_3D".to_string()),
            70.0,
        );
        record_gpu_engine_usage(
            &mut by_pid,
            &mut by_engine,
            Some(7),
            Some("luid_a_phys_0_eng_0_engtype_3D".to_string()),
            45.0,
        );

        assert_eq!(gpu_total_from_engines(&by_engine), 100.0);
    }

    #[test]
    fn process_action_rejects_current_process_pid() {
        let current_pid = std::process::id();

        assert!(validate_process_action_pid(current_pid).is_err());
    }

    #[test]
    fn explorer_select_arg_includes_target_path() {
        let path = PathBuf::from(r"C:\Program Files\App\app.exe");

        assert_eq!(
            explorer_select_arg(&path),
            r"/select,C:\Program Files\App\app.exe"
        );
    }

    #[test]
    fn resource_score_combines_weighted_usage_and_bottleneck() {
        let score = resource_score(20.0, 40.0, 80.0);

        assert!((score - 57.8).abs() < 0.001);
    }

    #[test]
    fn default_process_order_uses_resource_score() {
        let mut processes = vec![
            ProcessInfo {
                name: "cpu-heavy".to_string(),
                pid: 1,
                cpu_percent: 40.0,
                gpu_percent: 0.0,
                memory_percent: 0.0,
            },
            ProcessInfo {
                name: "balanced-heavy".to_string(),
                pid: 2,
                cpu_percent: 25.0,
                gpu_percent: 30.0,
                memory_percent: 35.0,
            },
        ];

        processes.sort_by(compare_by_resource_score);

        assert_eq!(processes[0].name, "balanced-heavy");
    }

    #[test]
    fn appearance_config_sanitizes_invalid_values() {
        let config = AppearanceConfig {
            opacity: 255,
            border_radius: 99,
            theme_mode: "neon".to_string(),
            accent_color: "red".to_string(),
            font_size: "huge".to_string(),
            background_blur: true,
            animations: false,
            window_shadow: true,
        }
        .sanitized();

        assert_eq!(config.opacity, 100);
        assert_eq!(config.border_radius, 32);
        assert_eq!(config.theme_mode, "system");
        assert_eq!(config.accent_color, "#7dd3fc");
        assert_eq!(config.font_size, "medium");
        assert!(config.background_blur);
        assert!(!config.animations);
        assert!(config.window_shadow);
    }

    #[test]
    fn appearance_config_merges_partial_user_config_with_defaults() {
        let config = appearance_config_from_json(r##"{"opacity":70,"accentColor":"#ff00aa"}"##);

        assert_eq!(config.opacity, 70);
        assert_eq!(config.accent_color, "#ff00aa");
        assert_eq!(config.border_radius, 12);
        assert_eq!(config.theme_mode, "system");
        assert_eq!(config.font_size, "medium");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_gpu_pid_reads_pdh_instance_names() {
        let mut name = wide(r"pid_1234_luid_0x00000000_0x00000000_phys_0_eng_0_engtype_3D");
        assert_eq!(parse_gpu_pid(name.as_mut_ptr()), Some(1234));
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
            apply_window_opacity,
            get_appearance_config,
            save_appearance_config,
            reset_appearance_config,
            terminate_process,
            open_process_location
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

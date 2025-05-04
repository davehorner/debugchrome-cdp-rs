use base64::Engine;
use rayon::prelude::*;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{env, fs, io};
use tungstenite::Message;

use futures_util::TryFutureExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// Global atomic counter for unique IDs
static COMMAND_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn get_unique_id() -> usize {
    COMMAND_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}
#[cfg(target_os = "windows")]
fn bring_chrome_to_front_and_resize_with_powershell(bounds: Option<(i32, i32, i32, i32)>) {
    let base_script = r#"
        $chrome = Get-Process chrome | Where-Object {
            $_.MainWindowHandle -ne 0 -and $_.Path -like '*chrome.exe'
        } | ForEach-Object {
            $cmdline = (Get-CimInstance Win32_Process -Filter "ProcessId=$($_.Id)").CommandLine
            if ($cmdline -like '*--remote-debugging-port=*') {
                $_
            }
        } | Select-Object -First 1

        if ($chrome) {
            $sig = @"
using System;
using System.Runtime.InteropServices;
public static class NativeMethods {
    [DllImport("user32.dll")]
    public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")]
    public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);
}
"@
            Add-Type -TypeDefinition $sig -Language CSharp | Out-Null

            $hWnd = $chrome.MainWindowHandle
            $currentThreadId = [NativeMethods]::GetWindowThreadProcessId([System.Diagnostics.Process]::GetCurrentProcess().MainWindowHandle, [ref]0)
            $chromeThreadId = [NativeMethods]::GetWindowThreadProcessId($hWnd, [ref]0)
            [NativeMethods]::AttachThreadInput($currentThreadId, $chromeThreadId, $true) | Out-Null
            [NativeMethods]::SetForegroundWindow($hWnd) | Out-Null
            [NativeMethods]::ShowWindowAsync($hWnd, 9) | Out-Null
            [NativeMethods]::AttachThreadInput($currentThreadId, $chromeThreadId, $false) | Out-Null
        }
    "#;

    let resize_script = if let Some((x, y, w, h)) = bounds {
        format!(
            r#"
            [NativeMethods]::MoveWindow($hWnd, {x}, {y}, {w}, {h}, $true) | Out-Null
            "#,
            x = x,
            y = y,
            w = w,
            h = h
        )
    } else {
        String::new()
    };

    let ps_script = format!("{}{}", base_script, resize_script);

    let _ = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(ps_script)
        .status();
}

fn split_and_process_url(raw_url: &str) -> (String, std::collections::HashMap<String, String>) {
    // Split the URL into the main part and the `!` parameters
    let mut parts = raw_url.splitn(2, "!");
    let base_url = parts.next().unwrap_or("").to_string();
    let mut bang_params = std::collections::HashMap::new();

    if let Some(bang_part) = parts.next() {
        // Split the `!` parameters and process them
        for param in bang_part.split('&') {
            if let Some((key, value)) = param.split_once('=') {
                bang_params.insert(key.trim_start_matches('!').to_string(), value.to_string());
            } else {
                // Handle flags like `!close` without `=`
                bang_params.insert(param.trim_start_matches('!').to_string(), String::new());
            }
        }
    }

    (base_url, bang_params)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Set the current working directory to the directory of the executing binary
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            std::env::set_current_dir(exe_dir)?;
            log::debug!("Working directory set to: {:?}", exe_dir);
        }
    }
    let log_file_path = "debugchrome.log";
    let log_file = File::create(log_file_path)?;
    WriteLogger::init(LevelFilter::Debug, Config::default(), log_file).unwrap();

    log::debug!("Starting Debug Chrome...");
    let log_file_path = std::fs::canonicalize(log_file_path)?.display().to_string();
    println!("Log file: {}", log_file_path);

    let args: Vec<String> = env::args().collect();
    if args.len() > 2 && args[1] == "--close-target" {
        let target_id = &args[2];
        let timeout_seconds: u64 = if let Some(arg) = args.get(4) {
            match arg.parse::<u64>() {
                Ok(value) => value,
                Err(_) => {
                    println!("Invalid timeout value provided: {}", arg);
                    0
                }
            }
        } else {
            0
        };

        log::debug!(
            "Waiting {} seconds before closing target {}...",
            timeout_seconds,
            target_id
        );
        std::thread::sleep(std::time::Duration::from_secs(timeout_seconds));

        if let Err(e) = close_tab_by_target_id(target_id).await {
            log::debug!("Failed to close target {}: {}", target_id, e);
        } else {
            log::debug!("Successfully closed target {}", target_id);
        }
        std::process::exit(0);
    }
    if args.len() > 1 && args[1] == "--register" {
        let exe_path = std::env::current_exe()?
            .to_str()
            .unwrap()
            .replace("\\", "\\\\");
        let reg_content = format!(
            "Windows Registry Editor Version 5.00\n\n\
            [HKEY_CLASSES_ROOT\\debugchrome]\n\
            @=\"URL:Debug Chrome Protocol\"\n\
            \"URL Protocol\"=\"\"\n\
            [HKEY_CLASSES_ROOT\\debugchrome\\shell\\open\\command]\n\
            @=\"\\\"{}\\\" \\\"%1\\\"\"\n",
            exe_path
        );
        let mut file = File::create("debugchrome.reg")?;
        file.write_all(reg_content.as_bytes())?;
        println!("Written debugchrome.reg with path: {}", exe_path);
        if let Err(e) = Command::new("regedit")
            .args(["/s", "debugchrome.reg"])
            .spawn()
            .and_then(|mut child| child.wait())
        {
            println!("Failed to register debugchrome protocol: {}", e);
            println!(
                "Try running this program in an elevated command prompt (Run as Administrator)."
            );
            println!("or double click the reg file.");
        } else {
            println!("Registered debugchrome protocol successfully.");
        }
        return Ok(());
    }
    let mut keep_focus = false;
    // Capture the current focused window if !keep_focus is set
    #[cfg(target_os = "windows")]
    let previous_window = get_focused_window();
    if args.len() > 2 && args[1] == "--search" {
        let search_id = &args[2];
        let close_tab = args.get(3).map(|arg| arg == "--close").unwrap_or(false);

        match search_tabs_for_bang_id(search_id).await {
            Ok(Some((target_id, title, url))) => {
                log::debug!("Found tab with bangId {}: {} ({})", search_id, title, url);

                if close_tab {
                    log::debug!("Closing tab with bangId {}...", search_id);
                    if let Err(e) = close_tab_by_target_id(&target_id).await {
                        log::debug!("Failed to close tab: {}", e);
                    } else {
                        log::debug!("Tab with bangId {} closed successfully.", search_id);
                    }
                }
            }
            Ok(None) => {
                log::debug!("No tab found with bangId = {}", search_id);
            }
            Err(e) => {
                log::debug!("Failed to search tabs: {}", e);
            }
        }
        #[cfg(target_os = "windows")]
        finalize_actions(previous_window, keep_focus);
        return Ok(());
    }

    if args.len() > 1 {
        let raw_url = &args[1];
        let translated = raw_url.replacen("debugchrome://", "", 1);
        let translated = translated.replacen("debugchrome:", "", 1);
        let (clean_url, bangs) = split_and_process_url(&translated);
        let user_data_dir = std::env::temp_dir().join("debugchrome");
        // Check if the !keep_focus parameter is present
        keep_focus = bangs.get("keep_focus").is_some();
        log::debug!("keep_focus: {}", keep_focus);

        // Check if the CDP server is running
        if !is_cdp_server_running().await {
            log::debug!(
                "CDP server is not running. Preparing Chrome profile and launching Chrome..."
            );

            // Prepare Chrome profile
            let user_data_dir = prepare_chrome_profile(true)?;
            log::debug!("User data cloned to: {}", user_data_dir.display());

            // Launch Chrome
            launch_chrome(&user_data_dir)?;
            log::debug!("Chrome launched successfully. Waiting for the CDP server to start...");
        } else {
            log::debug!("CDP server is already running.");
        }

        // Check if the bangId is already open
        let parsed_url = match url::Url::parse(&clean_url) {
            Ok(url) => url,
            Err(e) => {
                log::debug!("Failed to parse URL: {}", e);
                // sleep(std::time::Duration::from_secs(30)); // Ensure sleep even on error
                return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
            }
        };
        let open_window = bangs.get("openwindow").is_some();
        let close = bangs.get("close").is_some();
        let refresh = bangs.get("refresh").is_some();
        let screenshot = bangs.get("screenshot").is_some();
        let timeout_seconds = bangs.get("timeout").and_then(|v| v.parse::<u64>().ok());
        let monitor_index = bangs.get("monitor").and_then(|v| v.parse::<usize>().ok());
        #[cfg(target_os = "windows")]
        let bounds = get_screen_bounds(&bangs, monitor_index);
        #[cfg(not(target_os = "windows"))]
        let bounds: Option<(i32, i32, i32, i32)> = None;
        log::debug!("bangs: {:?}", bangs);
        log::debug!("Parsed URL: {}", parsed_url);
        if let Some(bang_id) = bangs.get("id").cloned() {
            log::debug!("Searching for bangId: {}", bang_id);
            if let Some((target_id, title, _url)) = search_tabs_for_bang_id(&bang_id)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                .await?
            {
                log::debug!(
                    "Tab with bangId {} title {} is already open: {}",
                    bang_id,
                    title,
                    target_id
                );

                // Activate the tab
                if let Err(e) = activate_tab(&target_id).await {
                    log::debug!("Failed to activate tab: {}", e);
                }
                if let Some((x, y, w, h)) = bounds {
                    println!("Setting window bounds: x={}, y={}, w={}, h={}", x, y, w, h);
                    #[cfg(target_os = "windows")]
                    set_window_bounds(&target_id, x, y, w, h).await.ok();
                    #[cfg(target_os = "windows")]
                    bring_chrome_to_front_and_resize_with_powershell(bounds);
                }

                // if let Err(e) = set_tab_title(&target_id, &target_id){
                //     log::debug!("Failed to set tab title: {}", e);
                // }
                // if let Some(hwnd) = find_chrome_hwnd_by_title(&target_id) {
                //     bring_hwnd_to_front(hwnd);
                // } else {
                //     log::debug!("Failed to find Chrome window with title '{}'.",&target_id);
                // }
                // set_tab_title(&target_id, &title).ok();
                if refresh {
                    log::debug!("Refreshing tab with bangId {}: {}", bang_id, target_id);
                    refresh_tab(&target_id).ok();
                }
                log::debug!(
                    "Tab with bangId {} is already open, activating it.",
                    bang_id
                );
                if close {
                    log::debug!("Closing tab with bangId {}...", target_id);
                    if let Err(e) = close_tab_by_target_id(&target_id).await {
                        log::debug!("Failed to close tab: {}", e);
                    } else {
                        log::debug!("Tab with bangId {} closed successfully.", target_id);
                    }
                }

                if let Some(timeout_seconds) = timeout_seconds {
                    log::debug!(
                        "Setting timeout of {} seconds to close target {}...",
                        timeout_seconds,
                        target_id
                    );
                    spawn_timeout_closer(target_id.clone(), timeout_seconds).ok();
                }
                #[cfg(target_os = "windows")]
                finalize_actions(previous_window, keep_focus);
                return Ok(());
            }
        }
        log::debug!("Tab with bangId {} not found, opening new tab.", translated);
        let result = if open_window {
            open_window_via_devtools(&clean_url, &bangs).await
        } else {
            open_tab_via_devtools_and_return_id(&clean_url, &bangs).await
        };
        #[cfg(target_os = "windows")]
        finalize_actions(previous_window, keep_focus);
        if let Ok(target_id) = result {
            if let Some((x, y, w, h)) = bounds {
                #[cfg(target_os = "windows")]
                set_window_bounds(&target_id, x, y, w, h).await.ok();
                #[cfg(target_os = "windows")]
                bring_chrome_to_front_and_resize_with_powershell(bounds);
            }
            // if let Some(hwnd) = find_chrome_hwnd_by_title(&target_id) {
            //     bring_hwnd_to_front(hwnd);
            // } else {
            //     log::debug!("Failed to find Chrome window with title '{}'.",&target_id);
            // }
            if screenshot {
                if let Err(e) = take_screenshot(&target_id) {
                    log::debug!("Failed to take screenshot: {}", e);
                    // std::thread::sleep(std::time::Duration::from_secs(3)); // Ensure sleep even on error
                    #[cfg(target_os = "windows")]
                    finalize_actions(previous_window, keep_focus);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{}", e),
                    ));
                }
            }

            // Call set_bang_id to set the bangId in the tab
            log::debug!("Setting bangId in the tab...{}", &translated);
            if let Err(e) = set_bang_id_session(&target_id, &translated) {
                log::debug!("Failed to set bangId: {}", e);
            }
            if let Some(timeout_seconds) = timeout_seconds {
                log::debug!(
                    "Setting timeout of {} seconds to close target {}...",
                    timeout_seconds,
                    target_id
                );
                spawn_timeout_closer(target_id.clone(), timeout_seconds).ok();
            }
        } else {
            Command::new("cmd")
                .args([
                    "/C",
                    "start",
                    "",
                    "chrome.exe",
                    "--remote-debugging-port=9222",
                    "--enable-automation",
                    "--no-first-run",
                    &format!("--user-data-dir={}", user_data_dir.display()),
                    &translated,
                ])
                .spawn()?;
        }

        log::debug!("Requested debug Chrome with URL: {}", translated);
    } else {
        println!("Usage:");
        println!(
            "  debugchrome.exe \"debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123\""
        );
        println!("  debugchrome.exe --search 123");
        println!("  debugchrome.exe --register");
    }
    #[cfg(target_os = "windows")]
    finalize_actions(previous_window, keep_focus);
    Ok(())
}
async fn open_window_via_devtools(
    clean_url: &str,
    _bangs: &std::collections::HashMap<String, String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::get("http://localhost:9222/json/version").await?;
    let version: serde_json::Value = response.json().await?;
    let ws_url = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("No WebSocket URL")?;
    let (socket, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let mut socket = socket;

    // Step 1: Create a new browser context
    let create_context = serde_json::json!({
        "id": 1,
        "method": "Target.createBrowserContext"
    });
    socket
        .send(tungstenite::Message::Text(
            create_context.to_string().into(),
        ))
        .await?;

    let browser_context_id = match socket.next().await {
        Some(Ok(tungstenite::Message::Text(txt))) => {
            let json: serde_json::Value = serde_json::from_str(&txt)?;
            json["result"]["browserContextId"]
                .as_str()
                .map(|s| s.to_string())
        }
        _ => None,
    };

    if let Some(context_id) = browser_context_id {
        // Step 2: Create a new target (window) in the new browser context
        let create_target = serde_json::json!({
            "id": 2,
            "method": "Target.createTarget",
            "params": {
                "url": clean_url,
                "browserContextId": context_id
            }
        });
        socket
            .send(tungstenite::Message::Text(create_target.to_string().into()))
            .await?;

        // Step 3: Wait for the response to get the targetId
        if let Some(Ok(tungstenite::Message::Text(txt))) = socket.next().await {
            let json: serde_json::Value = serde_json::from_str(&txt)?;
            if let Some(target_id) = json["result"]["targetId"].as_str() {
                return Ok(target_id.to_owned());
            }
        }
    }

    Err("Failed to create a new window".into())
}

fn get_screen_bounds(
    bangs: &std::collections::HashMap<String, String>,
    monitor_index: Option<usize>,
) -> Option<(i32, i32, i32, i32)> {
    #[cfg(not(target_os = "windows"))]
    {
        log::debug!("Screen bounds adjustment is only supported on Windows.");
        return None;
    }
    #[cfg(target_os = "windows")]
    {
        let (screen_width, screen_height) = get_screen_resolution(monitor_index);

        let x = bangs
            .get("x")
            .and_then(|v| parse_dimension(v, screen_width))
            .unwrap_or(0);
        let y = bangs
            .get("y")
            .and_then(|v| parse_dimension(v, screen_height))
            .unwrap_or(0);
        let w = bangs
            .get("w")
            .and_then(|v| parse_dimension(v, screen_width))
            .unwrap_or(1024);
        let h = bangs
            .get("h")
            .and_then(|v| parse_dimension(v, screen_height))
            .unwrap_or(768);
        if let Some(index) = monitor_index {
            if let Some((adjusted_x, adjusted_y, adjusted_w, adjusted_h)) =
                adjust_bounds_to_monitor(index, x, y, w, h)
            {
                log::debug!(
                    "Adjusted bounds to monitor {}: x={}, y={}, w={}, h={}",
                    index,
                    adjusted_x,
                    adjusted_y,
                    adjusted_w,
                    adjusted_h
                );
                return Some((adjusted_x, adjusted_y, adjusted_w, adjusted_h));
            }
        }
        log::debug!("x: {}, y: {}, w: {}, h: {}", x, y, w, h);
        if bangs.contains_key("x")
            && bangs.contains_key("y")
            && bangs.contains_key("w")
            && bangs.contains_key("h")
        {
            Some((x, y, w, h))
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn finalize_actions(previous_window: Option<winapi::shared::windef::HWND>, keep_focus: bool) {
    log::debug!("Finalizing actions...{} {:?}", keep_focus, previous_window);
    // Restore focus to the previous window if !keep_focus is set
    if keep_focus {
        if let Some(hwnd) = previous_window {
            log::debug!("Restoring focus to the previously focused window...");
            unsafe {
                SetForegroundWindow(hwnd);
            }
        }
    }
}

async fn open_tab_via_devtools_and_return_id(
    clean_url: &str,
    _bangs: &std::collections::HashMap<String, String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::get("http://localhost:9222/json/version").await?;
    let version: serde_json::Value = response.json().await?;
    let ws_url = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("No WebSocket URL")?;
    let (socket, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let mut socket = socket;

    let msg = serde_json::json!({
        "id": 1,
        "method": "Target.createTarget",
        "params": { "url": clean_url }
    });

    socket
        .send(tungstenite::Message::Text(msg.to_string().into()))
        .await?;

    let timeout = std::time::Duration::from_secs(5); // Define a timeout duration
    match tokio::time::timeout(timeout, socket.next()).await {
        Ok(Some(msg)) => {
            if let Ok(Message::Text(txt)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if let Some(target_id) = json["result"]["targetId"].as_str() {
                        return Ok(target_id.to_owned());
                    }
                }
            }
        }
        Ok(None) => {
            log::debug!("WebSocket stream ended unexpectedly.");
        }
        Err(_) => {
            log::debug!("Timeout while reading from WebSocket.");
        }
    }

    Err("Failed to get targetId".into())
}

async fn set_window_bounds(
    target_id: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    // let window_id_resp: serde_json::Value = reqwest::get("http://localhost:9222/json").await?
    //     .json().await?;

    let version: serde_json::Value = reqwest::get("http://localhost:9222/json/version")
        .await?
        .json()
        .await?;
    let ws_url = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("No WebSocket URL")?;
    let (mut socket, _) = tungstenite::connect(ws_url)?;

    let get_window = serde_json::json!({
        "id": 3,
        "method": "Browser.getWindowForTarget",
        "params": {
            "targetId": target_id
        }
    });
    socket.send(tungstenite::Message::Text(get_window.to_string().into()))?;

    let mut window_id: Option<i32> = None;
    while let Ok(msg) = socket.read() {
        if let tungstenite::Message::Text(txt) = msg {
            let json: serde_json::Value = serde_json::from_str(&txt)?;
            if let Some(id) = json["result"]["windowId"].as_i64() {
                window_id = Some(id as i32);
                break;
            }
        }
    }

    if let Some(id) = window_id {
        let bounds = serde_json::json!({
            "id": 4,
            "method": "Browser.setWindowBounds",
            "params": {
                "windowId": id,
                "bounds": { "left": x, "top": y, "width": w, "height": h }
            }
        });
        socket.send(tungstenite::Message::Text(bounds.to_string().into()))?;
    }

    Ok(())
}

fn take_screenshot(target_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = tungstenite::connect(&socket_url)?;

    let enable = serde_json::json!({
        "id": 1,
        "method": "Page.enable"
    });
    socket.send(tungstenite::Message::Text(enable.to_string().into()))?;

    let capture = serde_json::json!({
        "id": 2,
        "method": "Page.captureScreenshot"
    });
    log::debug!("Sending captureScreenshot command: {}", capture);
    socket.send(tungstenite::Message::Text(capture.to_string().into()))?;

    log::debug!("Current directory: {:?}", std::env::current_dir()?);
    while let Ok(msg) = socket.read() {
        if let tungstenite::Message::Text(txt) = msg {
            let json: serde_json::Value = serde_json::from_str(&txt)?;
            if let Some(data) = json["result"]["data"].as_str() {
                let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
                std::fs::write("screenshot.png", bytes)?;
                log::debug!("Screenshot saved to screenshot.png");
                break;
            }
        }
    }
    //std::thread::sleep(std::time::Duration::from_secs(30));
    Ok(())
}

async fn search_tabs_for_bang_id(
    search_id: &str,
) -> Result<Option<(String, String, String)>, Box<dyn std::error::Error + Send + Sync>> {
    log::debug!("Searching for bangId = {}", search_id);
    let uses_session = true;
    // Fetch the list of tabs
    let response = reqwest::get("http://localhost:9222/json").await?;
    let tabs: Vec<serde_json::Value> = response.json().await?;
    let results = Arc::new(std::sync::Mutex::new(None)); // Shared result storage

    // Process tabs in parallel using rayon
    tabs.par_iter()
        .map(|tab| {
            let tab_url = tab["url"].as_str().unwrap_or("<no url>");
            let target_id = tab["id"].as_str().unwrap_or("<no id>").to_string();
            let title = tab["title"].as_str().unwrap_or("<no title>").to_string();
            let page_url = tab["url"].as_str().unwrap_or("<no url>").to_string();

            if is_invalid_url(&page_url) {
                return;
            }

            log::debug!("Searching tab: {}", tab_url);
            if let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() {
                let results = Arc::clone(&results);

                // Use a timeout for the WebSocket operation
                let start_time = std::time::Instant::now();
                let timeout = Duration::from_secs(5);

                if let Ok((mut socket, _)) = tungstenite::connect(ws_url) {
                    log::debug!("Connected to WebSocket URL: {}", ws_url);

                    // Generate a unique ID for this command
                    let command_id = get_unique_id();

                    // Send the Runtime.evaluate command to get window.bangId
                    let get_bang_id = if !uses_session {
                    serde_json::json!({
                        "id": command_id,
                        "method": "Runtime.evaluate",
                        "params": {
                            "expression": "window.bangId"
                        }
                    })
                } else {
                    serde_json::json!({
                        "id": command_id,
                        "method": "Runtime.evaluate",
                        "params": {
                            "expression": "sessionStorage.getItem('bangId')",
                            "returnByValue": true
                        }
                    })
                };
                    if socket.send(Message::Text(get_bang_id.to_string().into())).is_ok() {
                        log::debug!("Sent command to get bangId with id {} {:?}", command_id, get_bang_id);

                        // Wait for a response with a timeout
                        while start_time.elapsed() < timeout {
                            if socket.can_read() {
                            if let Ok(msg) = socket.read() {
                                if let Message::Text(txt) = msg {
                                    log::debug!("Received message: {}", txt);
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                                        if json["id"] == command_id {
                                            if let Some(bang_id) = json["result"]["result"]["value"].as_str() {
                                                if bang_id == search_id {
                                                    log::debug!(
                                                        "Found tab with bangId {}: {}",
                                                        search_id,
                                                        tab_url
                                                    );

                                                    // Store the result and exit
                                                    match results.lock() {
                                                        Ok(mut results) => {
                                                            *results = Some((target_id, title, page_url));
                                                            return;
                                                        }
                                                        Err(_) => {
                                                            log::debug!("Failed to acquire lock on results");
                                                        }
                                                    }
                                                }
                                            }
                                            break; // Exit loop after processing the response
                                        }
                                    }
                                }
                            }
                            }
                        }
                    }
                }

                log::debug!(
                    "{} or no matching response while searching tab with WebSocket URL: {}",
                    ws_url,title
                );
            }
        })
        .collect::<Vec<_>>(); // Wait for all tasks to complete

    // Check if a result was found
    if let Some(url) = &*results.lock().unwrap() {
        log::debug!("Found tab with bangId {}: {:?}", search_id, url);
        return Ok(Some(url.clone()));
    } else {
        log::debug!("No tab found with bangId = {}", search_id);
    }

    Ok(None)
}

async fn activate_tab(target_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Fetch the WebSocket debugger URL
    let version: serde_json::Value = reqwest::get("http://localhost:9222/json/version")
        .await?
        .json()
        .await?;
    let ws_url = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("No WebSocket URL")?;

    // Connect to the WebSocket
    let (mut socket, _) = tungstenite::connect(ws_url)?;

    // Send the Target.activateTarget command
    let activate_command = serde_json::json!({
        "id": get_unique_id(),
        "method": "Target.activateTarget",
        "params": { "targetId": target_id }
    });
    socket.send(Message::Text(activate_command.to_string().into()))?;
    log::debug!("Activated tab with targetId: {}", target_id);

    Ok(())
}

fn is_invalid_url(url: &str) -> bool {
    // List of URL prefixes or patterns to exclude
    let invalid_prefixes = [
        "ws://",               // WebSocket URLs
        "chrome-extension://", // Chrome extensions
        "chrome://",           // Internal Chrome pages
        "chrome-untrusted://", // Internal Chrome pages
        "about:",              // About pages
        "file://",             // Local file URLs
        "data:",               // Data URLs
        "javascript:",         // JavaScript URLs
    ];

    // Check if the URL starts with any of the invalid prefixes
    invalid_prefixes
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

#[cfg(target_os = "windows")]
use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

#[cfg(target_os = "windows")]
fn get_screen_resolution(monitor_index: Option<usize>) -> (i32, i32) {
    let monitors = get_monitor_bounds();
    if monitors.is_empty() {
        // Fallback to primary screen resolution if no monitors are detected
        let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        (width, height)
    } else {
        // Handle out-of-bounds monitor index and use the resolution of the specified monitor
        let monitor = monitors.get(monitor_index.unwrap_or(0)).unwrap_or_else(|| {
            log::debug!("Monitor index out of bounds, falling back to primary monitor.");
            &monitors[0]
        });
        let width = monitor.right - monitor.left;
        let height = monitor.bottom - monitor.top;
        (width, height)
    }
}

fn parse_dimension(value: &str, max: i32) -> Option<i32> {
    if value.ends_with('%') {
        // Parse as percentage
        let percentage = value.trim_end_matches('%').parse::<f32>().ok()?;
        Some(((percentage / 100.0) * max as f32).round() as i32)
    } else {
        // Parse as absolute value
        value.parse::<i32>().ok()
    }
}

use std::ptr;
#[cfg(target_os = "windows")]
use winapi::shared::windef::HWND;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{EnumWindows, GetWindowTextA};

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn find_chrome_hwnd_by_title(title: &str) -> Option<HWND> {
    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: isize) -> i32 {
        unsafe {
            let data = &mut *(lparam as *mut (String, HWND));
            let title_ptr = &data.0;
            let hwnd_ptr = &mut data.1;

            let mut buffer = [0; 256];
            let length = GetWindowTextA(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);

            if length > 0 {
                let window_title = String::from_utf8_lossy(std::slice::from_raw_parts(
                    buffer.as_ptr().cast::<u8>(),
                    length as usize,
                ))
                .to_string();
                log::debug!("Window title: {}", window_title);
                if window_title.contains(title_ptr) {
                    //&& IsWindowVisible(hwnd) != 0 {
                    *hwnd_ptr = hwnd;
                    return 0; // Stop enumeration
                }
            }
            1 // Continue enumeration
        }
    }

    let hwnd: HWND = ptr::null_mut();
    let mut data = (title.to_string(), hwnd);
    unsafe {
        EnumWindows(Some(enum_windows_proc), &mut data as *mut _ as isize);
    }

    if data.1.is_null() { None } else { Some(data.1) }
}

#[allow(dead_code)]
fn set_tab_title(target_id: &str, new_title: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = tungstenite::connect(&socket_url)?;
    let enable = serde_json::json!({
        "id": 1,
        "method": "Runtime.enable"
    });
    socket.write(Message::Text(enable.to_string().into()))?;
    // JavaScript to set the document title
    let id = get_unique_id();
    let set_title_script = format!("document.title = '{}';", new_title);
    let set_title_command = serde_json::json!({
        "id": id,
        "method": "Runtime.evaluate",
        "params": {
            "expression": set_title_script
        }
    });

    socket.send(Message::Text(set_title_command.to_string().into()))?;
    // 5) **Drain** until we see our eval response

    match socket.read()? {
        Message::Text(txt) => {
            if let Ok(msg) = serde_json::from_str::<Value>(&txt) {
                // look for our eval_id
                if msg["id"].as_i64() == Some(id.try_into().unwrap()) {
                    log::debug!("✅ title set response: {}", txt);
                }
            }
        }
        _ => {}
    }

    log::debug!("{:?}", socket.read()?); // Read the response
    log::debug!("Set tab {} title to: {}", target_id, new_title);

    Ok(())
}

use futures_util::{SinkExt, StreamExt};
#[cfg(target_os = "windows")]
use winapi::um::winuser::{SW_RESTORE, SetForegroundWindow, ShowWindow};

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn bring_hwnd_to_front(hwnd: HWND) {
    if hwnd.is_null() {
        log::debug!("Invalid HWND: Cannot bring to front.");
        return;
    }

    unsafe {
        // Restore the window if it is minimized
        ShowWindow(hwnd, SW_RESTORE);
        // Bring the window to the foreground
        SetForegroundWindow(hwnd);
    }
}

async fn is_cdp_server_running() -> bool {
    match reqwest::get("http://localhost:9222/json").await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

fn prepare_chrome_profile(new_environment: bool) -> io::Result<std::path::PathBuf> {
    let chrome_user_data = dirs::data_local_dir()
        .expect("%LOCALAPPDATA% not found")
        .join("Google\\Chrome\\User Data");

    let source_default = chrome_user_data.join("Default");
    let source_local_state = chrome_user_data.join("Local State");
    let source_sessions = source_default.join("Sessions");

    let temp_root = if new_environment {
        let timestamp = chrono::Local::now()
            .format("debugchrome-%y%m%d%H%M%S")
            .to_string();
        env::temp_dir().join(timestamp)
    } else {
        env::temp_dir().join("debugchrome")
    };

    let temp_default = temp_root.join("Default");
    let temp_sessions = temp_default.join("Sessions");

    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_default)?;

    if new_environment {
        // Only copy Local State for new environment setup
        Command::new("xcopy")
            .arg(&source_local_state)
            .arg(&temp_root)
            .args(["/H", "/Y"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
    } else {
        // Full copy of Default and Sessions
        fs::create_dir_all(&temp_sessions)?;

        Command::new("xcopy")
            .arg(&source_default)
            .arg(&temp_default)
            .args(["/E", "/I", "/H", "/Y"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;

        Command::new("xcopy")
            .arg(&source_local_state)
            .arg(&temp_root)
            .args(["/H", "/Y"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;

        for entry in fs::read_dir(&source_sessions)? {
            let path = entry?.path();
            if path.is_file() && fs::metadata(&path)?.len() > 0 {
                let filename = path.file_name().unwrap();
                fs::copy(&path, temp_sessions.join(filename))?;
            }
        }
    }

    Ok(temp_root)
}

fn launch_chrome(user_data_dir: &Path) -> io::Result<()> {
    Command::new("cmd")
        .args([
            "/C",
            "start",
            "chrome.exe",
            "--remote-debugging-port=9222",
            "--enable-automation",
            "--no-first-run",
            &format!("--user-data-dir={}", user_data_dir.display()),
        ])
        .spawn()?;
    Ok(())
}

fn refresh_tab(target_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = tungstenite::connect(&socket_url)?;

    // Send the Page.reload command
    let reload_command = serde_json::json!({
        "id": 1,
        "method": "Page.reload",
        "params": {}
    });

    socket.send(Message::Text(reload_command.to_string().into()))?;
    log::debug!("Sent command to refresh tab with targetId: {}", target_id);

    // Optionally, wait for a response to confirm the reload
    if let Ok(msg) = socket.read() {
        if let Message::Text(txt) = msg {
            log::debug!("Received response: {}", txt);
        }
    }

    Ok(())
}

async fn close_tab_by_target_id(target_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let version: serde_json::Value = reqwest::get("http://localhost:9222/json/version")
        .await?
        .json()
        .await?;
    let ws_url = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("No WebSocket URL")?;
    let (mut socket, _) = tungstenite::connect(ws_url)?;

    let close_command = serde_json::json!({
        "id": get_unique_id(),
        "method": "Target.closeTarget",
        "params": { "targetId": target_id }
    });

    socket.send(Message::Text(close_command.to_string().into()))?;
    log::debug!("Sent command to close tab with targetId: {}", target_id);

    Ok(())
}

fn set_bang_id_session(target_id: &str, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = url::Url::parse(url)?;
    if let Some(bang_id) = parsed
        .query_pairs()
        .find(|(k, _)| k == "!id")
        .map(|(_, v)| v.to_string())
    {
        let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
        let (mut socket, _) = tungstenite::connect(&socket_url)?;

        // Set the bangId in sessionStorage
        let set_bang_id = serde_json::json!({
            "id": 3,
            "method": "Runtime.evaluate",
            "params": {
                "expression": format!("sessionStorage.setItem('bangId', '{}');", bang_id),
            }
        });
        socket.send(Message::Text(set_bang_id.to_string().into()))?;
        log::debug!("Set sessionStorage.bangId to {}", bang_id);

        // Verify that the bangId was set
        let verify_bang_id = serde_json::json!({
            "id": 4,
            "method": "Runtime.evaluate",
            "params": {
                "expression": "sessionStorage.getItem('bangId')",
            }
        });
        socket.send(Message::Text(verify_bang_id.to_string().into()))?;
        log::debug!("Sent command to verify bangId");

        // Wait for the response
        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);
        while start_time.elapsed() < timeout {
            if let Ok(msg) = socket.read() {
                if let Message::Text(txt) = msg {
                    log::debug!("Received message: {}", txt);
                    let json: serde_json::Value = serde_json::from_str(&txt)?;
                    if json["id"] == 4 {
                        if let Some(verified_bang_id) = json["result"]["result"]["value"].as_str() {
                            if verified_bang_id == bang_id {
                                log::debug!("Successfully verified bangId: {}", verified_bang_id);
                                return Ok(());
                            } else {
                                log::debug!(
                                    "Mismatch: Expected {}, but got {}",
                                    bang_id,
                                    verified_bang_id
                                );
                                return Err("Failed to verify bangId".into());
                            }
                        }
                    }
                }
            }
        }

        log::debug!("Timeout while verifying bangId");
        return Err("Timeout while verifying bangId".into());
    }
    Ok(())
}

fn spawn_timeout_closer(
    target_id: String,
    timeout_seconds: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let args = [
        "/C",
        "start",
        "/B",
        current_exe.to_str().unwrap(),
        "--close-target",
        &target_id,
        "--timeout",
        &timeout_seconds.to_string(),
    ];
    log::debug!("Spawning detached process with args: cmd {:?}", args);

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(args)
            .creation_flags(0x08000000)
            .spawn()?;
    }

    log::debug!(
        "Spawned detached process to close target {} after {} seconds.",
        target_id,
        timeout_seconds
    );

    Ok(())
}

use log::LevelFilter;
use simplelog::{Config, WriteLogger};
#[cfg(target_os = "windows")]
use winapi::um::winuser::GetForegroundWindow;

#[cfg(target_os = "windows")]
fn get_focused_window() -> Option<winapi::shared::windef::HWND> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() { None } else { Some(hwnd) }
    }
}
#[cfg(target_os = "windows")]
use winapi::shared::windef::{HMONITOR, RECT};
#[cfg(target_os = "windows")]
use winapi::um::winuser::EnumDisplayMonitors;

#[cfg(target_os = "windows")]
fn get_monitor_bounds() -> Vec<RECT> {
    let mut monitors = Vec::new();

    unsafe extern "system" fn monitor_enum_proc(
        _hmonitor: HMONITOR,
        _: *mut winapi::shared::windef::HDC__,
        lprc_monitor: *mut RECT,
        lparam: isize,
    ) -> i32 {
        let monitors = unsafe { &mut *(lparam as *mut Vec<RECT>) };
        monitors.push(unsafe { *lprc_monitor });
        1 // Continue enumeration
    }

    unsafe {
        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null_mut(),
            Some(monitor_enum_proc),
            &mut monitors as *mut _ as isize,
        );
    }

    monitors
}

fn adjust_bounds_to_monitor(
    monitor_index: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Option<(i32, i32, i32, i32)> {
    #[cfg(not(target_os = "windows"))]
    {
        log::debug!("Screen bounds adjustment is only supported on Windows.");
        return None;
    }
    #[cfg(target_os = "windows")]
    {
        let monitors = get_monitor_bounds();
        if monitor_index >= monitors.len() {
            return None; // Invalid monitor index
        }

        let monitor = monitors[monitor_index];
        let monitor_width = monitor.right - monitor.left;
        let monitor_height = monitor.bottom - monitor.top;

        // Adjust bounds relative to the monitor
        let adjusted_x = monitor.left + x;
        let adjusted_y = monitor.top + y;
        let adjusted_w = w.min(monitor_width);
        let adjusted_h = h.min(monitor_height);

        Some((adjusted_x, adjusted_y, adjusted_w, adjusted_h))
    }
}

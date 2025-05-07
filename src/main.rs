use base64::Engine;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{env, fs, io};
use tungstenite::Message;

use futures_util::TryFutureExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
struct MonitorInfo {
    hmonitor: HMONITOR,
    rect: RECT,
    dpi_scaling: f32,
}

#[cfg(feature = "uses_gui")]
mod gui;
#[cfg(feature = "uses_funny")]
mod jokes;

#[cfg(target_os = "windows")]
impl std::fmt::Debug for MonitorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MonitorInfo {{ hmonitor: {:?}, rect: {{ left: {}, top: {}, right: {}, bottom: {} }}, dpi_scaling: {} }}",
            self.hmonitor,
            self.rect.left,
            self.rect.top,
            self.rect.right,
            self.rect.bottom,
            self.dpi_scaling
        )
    }
}

// Global atomic counter for unique IDs
static COMMAND_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn get_unique_id() -> usize {
    COMMAND_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}
#[cfg(target_os = "windows")]
#[allow(dead_code)]
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
    let args: Vec<String> = env::args().collect();
    // Set the current working directory to the directory of the executing binary
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            std::env::set_current_dir(exe_dir)?;
            log::debug!("Working directory set to: {:?}", exe_dir);
        }
    }
    let log_file_path = "debugchrome.log";
    let append_log = if let Ok(metadata) = fs::metadata(log_file_path) {
        metadata.len() <= 5 * 1024 * 1024 // Check if the file size is 5 MB or less
    } else {
        true // Default to append if metadata cannot be retrieved
    };

    let log_file = if append_log {
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(log_file_path)?
    } else {
        File::create(log_file_path)?
    };
    WriteLogger::init(LevelFilter::Debug, Config::default(), log_file).unwrap();
    log::debug!("DebugChrome started with args: {:?}", args);
    setup_panic_hook(); // must be done after logger initialization

    if args.len() > 1 {
        let raw_url = &args[1];
        if raw_url == "debugchrome:"
            || raw_url == "debugchrome:/"
            || raw_url == "debugchrome://"
            || raw_url == "debugchrome:///"
        {
            #[cfg(feature = "uses_gui")]
            {
                println!("Starting GUI...");
                if let Err(e) = gui::start_gui().await {
                    eprintln!("GUI error: {}", e);
                    log::error!("GUI error: {}", e);
                    std::process::exit(1);
                }
            }

            #[cfg(not(feature = "uses_gui"))]
            {
                eprintln!("GUI support is not enabled. Rebuild with the `uses_gui` feature.");
                log::error!("GUI support is not enabled. Rebuild with the `uses_gui` feature.");
                std::process::exit(1);
            }
            std::process::exit(0);
        }

        log::debug!("Received URL: {}", raw_url);
        println!("Received URL: {:?}", raw_url);
    }
    let log_file_path = std::fs::canonicalize(log_file_path)?.display().to_string();
    println!("Log file: {}", log_file_path);

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
            println!("Press Enter to run an elevated powershell or Ctrl+C to exit.");
            let mut input = String::new();
            let _ = std::io::stdin().read_line(&mut input);
            if let Err(e) = Command::new("powershell")
                .args([
                    "-Command",
                    "Start-Process",
                    "powershell",
                    "-ArgumentList",
                    &format!("'{}'", "regedit /s debugchrome.reg"),
                    "-Verb",
                    "runAs",
                ])
                .spawn()
                .and_then(|mut child| child.wait())
            {
                println!("Failed to elevate and register debugchrome protocol: {}", e);
            } else {
                println!("Registered debugchrome protocol successfully with elevation.");
            }
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
        let dpi_scaling_enabled = bangs
            .get("dpi")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        log::debug!("DPI scaling enabled: {}", dpi_scaling_enabled);

        #[cfg(target_os = "windows")]
        let bounds = get_screen_bounds(&bangs, monitor_index, dpi_scaling_enabled);
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
                    refresh_tab(&target_id).await.ok();
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
        log::debug!("{} not found, opening.", clean_url);
        let result = if open_window {
            open_window_via_devtools(&clean_url, &bangs).await
        } else {
            open_tab_via_devtools_and_return_id(&clean_url, &bangs).await
        };
        #[cfg(target_os = "windows")]
        finalize_actions(previous_window, keep_focus);
        if let Ok(target_id) = result {
            // if let Some((x, y, w, h)) = bounds {
            //     #[cfg(target_os = "windows")]
            //     set_window_bounds(&target_id, x, y, w, h).await.ok();
            //     #[cfg(target_os = "windows")]
            //     bring_chrome_to_front_and_resize_with_powershell(bounds);
            // }
            // if let Some(hwnd) = find_chrome_hwnd_by_title(&target_id) {
            //     bring_hwnd_to_front(hwnd);
            // } else {
            //     log::debug!("Failed to find Chrome window with title '{}'.",&target_id);
            // }
            if screenshot {
                if let Err(e) = take_screenshot(&target_id).await {
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
            log::debug!("Setting bangId in the tab...{}", &clean_url);
            if let Err(e) =
                set_bang_id_session(&target_id, &bangs.get("id").cloned().unwrap_or_default()).await
            {
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
            let window_position = if let Some((x, y, _, _)) = bounds {
                Some(format!("--window-position={},{}", x, y))
            } else {
                None
            };

            let window_size = if let Some((_, _, w, h)) = bounds {
                Some(format!("--window-size={},{}", w, h))
            } else {
                None
            };

            let mut args = String::from(
                "/C start  chrome.exe --remote-debugging-port=9222 --enable-automation --no-first-run",
            );
            args.push_str(&format!(
                " --user-data-dir={} {}",
                user_data_dir.display(),
                clean_url
            ));

            if let Some(position) = window_position {
                args.push_str(&format!(" {}", position));
            }

            if let Some(size) = window_size {
                args.push_str(&format!(" {}", size));
            }

            Command::new("cmd").args(args.split_whitespace()).spawn()?;
        }

        log::debug!("Requested debug Chrome with URL: {}", translated);
    } else {
        println!("Usage:");
        println!(
            "  debugchrome.exe \"debugchrome:https://www.rust-lang.org?!x=0&!y=0&!w=800&!h=600&!id=123\""
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
    bangs: &std::collections::HashMap<String, String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::get("http://localhost:9222/json/version").await?;
    let version: serde_json::Value = response.json().await?;
    let ws_url = version["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("No WebSocket URL")?;
    let (socket, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let mut socket = socket;
    // Step 1: Create a new browser context
    let unique = get_unique_id();
    let create_context = serde_json::json!({
        "id": unique,
        "method": "Target.createBrowserContext"
    });
    socket
        .send(tungstenite::Message::Text(
            create_context.to_string().into(),
        ))
        .await?;

    let browser_context_id = match tokio::time::timeout(Duration::from_secs(5), socket.next()).await
    {
        Ok(Some(Ok(tungstenite::Message::Text(txt)))) => {
            let json: serde_json::Value = serde_json::from_str(&txt)?;
            json["result"]["browserContextId"]
                .as_str()
                .map(|s| s.to_string())
        }
        _ => None,
    };
    let monitor_index = bangs.get("monitor").and_then(|v| v.parse::<usize>().ok());
    let dpi_scaling_enabled = bangs
        .get("dpi")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let (left, top, width, height, include_bounds) =
        if let Some(bounds) = get_screen_bounds(&bangs, monitor_index, dpi_scaling_enabled) {
            (bounds.0, bounds.1, bounds.2, bounds.3, true)
        } else {
            (0, 0, 0, 0, false) // Indicate that bounds should not be included
        };

    use base64::engine::general_purpose::STANDARD as base64_engine;

    let bang_id = bangs.get("id").cloned().unwrap_or_default();
    let html_content = include_str!("../static/initial_payload.html")
        .replace("{{BANG_ID}}", &bang_id)
        .replace("{{CLEAN_URL}}", clean_url)
        .replace("{{DELAY_IN_SECONDS}}", "2");
    let encoded_html = base64_engine.encode(html_content);
    let placeholder_url = format!("data:text/html;base64,{}#{}", encoded_html, bang_id);
    println!(
        "{:?} Bounds: left={}, top={}, width={}, height={}, include_bounds={}",
        monitor_index, left, top, width, height, include_bounds
    );
    let unique = get_unique_id();
    if let Some(context_id) = browser_context_id {
        if include_bounds {
            let create_target = serde_json::json!({
                "id": unique,
                "method": "Target.createTarget",
                "params": {
                    "url": placeholder_url,
                    // "url": clean_url,
                    "browserContextId": context_id,
                    "left": left,
                    "top": top,
                    "width": width,
                    "height": height,
                    "newWindow": true
                }
            });
            socket
                .send(tungstenite::Message::Text(create_target.to_string().into()))
                .await?;
        } else {
            let create_target = serde_json::json!({
                "id": unique,
                "method": "Target.createTarget",
                "params": {
                    "url": placeholder_url,
                    // "url": clean_url,
                    "browserContextId": context_id,
                    "newWindow": true
                }
            });
            socket
                .send(tungstenite::Message::Text(create_target.to_string().into()))
                .await?;
        }
        // // Step 2: Create a new target (window) in the new browser context
        // let create_target = serde_json::json!({
        //     "id": 2,
        //     "method": "Target.createTarget",
        //     "params": {
        //         "url": clean_url,
        //         "browserContextId": context_id,
        //         "left": left,
        //         "top": top,
        //         "width": width,
        //         "height": height,
        //         "newWindow": true
        //     }
        // });
        // socket
        //     .send(tungstenite::Message::Text(create_target.to_string().into()))
        //     .await?;

        // Step 3: Wait for the response to get the targetId
        let timeout = std::time::Duration::from_secs(5); // Define a timeout duration
        if let Ok(Some(Ok(tungstenite::Message::Text(txt)))) =
            tokio::time::timeout(timeout, socket.next()).await
        {
            let json: serde_json::Value = serde_json::from_str(&txt)?;
            if let Some(target_id) = json["result"]["targetId"].as_str() {
                let _ =
                    set_bang_id_session(&target_id, &bangs.get("id").cloned().unwrap_or_default())
                        .await;
                return Ok(target_id.to_owned());
            }
        }
    }

    Err("Failed to create a new window".into())
}

fn get_screen_bounds(
    bangs: &std::collections::HashMap<String, String>,
    monitor_index: Option<usize>,
    dpi_scaling_enabled: bool,
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
            .unwrap_or(screen_width);
        let h = bangs
            .get("h")
            .and_then(|v| parse_dimension(v, screen_height))
            .unwrap_or(screen_height);
        print!("params: x: {}, y: {}, w: {}, h: {}", x, y, w, h);
        if let Some(index) = monitor_index {
            println!(" monitor_index: {}", index);
            if let Some((adjusted_x, adjusted_y, adjusted_w, adjusted_h)) =
                adjust_bounds_to_monitor(index, x, y, w, h, dpi_scaling_enabled)
            {
                println!(
                    "Adjusted bounds to monitor {}: x={}, y={}, w={}, h={}",
                    index, adjusted_x, adjusted_y, adjusted_w, adjusted_h
                );
                return Some((adjusted_x, adjusted_y, adjusted_w, adjusted_h));
            }
        }
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
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = tokio_tungstenite::connect_async(&socket_url).await?;

    let bounds = serde_json::json!({
        "id": 4,
        "method": "Browser.setWindowBounds",
        "params": {
            "windowId": target_id,
            "bounds": { "left": x, "top": y, "width": w, "height": h }
        }
    });
    socket
        .send(tungstenite::Message::Text(bounds.to_string().into()))
        .await?;
    Ok(())
}

async fn take_screenshot(target_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = tokio_tungstenite::connect_async(&socket_url).await?;
    let enable = serde_json::json!({
        "id": 1,
        "method": "Page.enable"
    });
    socket
        .send(tungstenite::Message::Text(enable.to_string().into()))
        .await?;

    let capture = serde_json::json!({
        "id": 2,
        "method": "Page.captureScreenshot"
    });
    log::debug!("Sending captureScreenshot command: {}", capture);
    socket
        .send(tungstenite::Message::Text(capture.to_string().into()))
        .await?;

    log::debug!("Current directory: {:?}", std::env::current_dir()?);
    let timeout_duration = Duration::from_secs(5); // Set a timeout duration
    let timeout_future = tokio::time::sleep(timeout_duration);
    tokio::pin!(timeout_future);

    'outer: while let Some(Ok(tungstenite::Message::Text(txt))) = tokio::select! {
        _ = &mut timeout_future => {
            log::debug!("Timeout while waiting for screenshot response.");
            break 'outer;
        }
        msg = socket.next() => msg
    } {
        let json: serde_json::Value = serde_json::from_str(&txt)?;
        if let Some(data) = json["result"]["data"].as_str() {
            let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
            std::fs::write("debugchrome.png", bytes)?;
            Command::new("powershell")
                .args(["-NoProfile", "-Command", "Start-Process debugchrome.png"])
                .status()
                .ok();
            log::debug!("Screenshot saved to debugchrome.png");
            break;
        }
    }
    Ok(())
}

use futures::stream::{FuturesUnordered, StreamExt};
use tokio_tungstenite::connect_async;

async fn search_tabs_for_bang_id(
    search_id: &str,
) -> Result<Option<(String, String, String)>, Box<dyn std::error::Error + Send + Sync>> {
    log::debug!("Searching for bangId = {}", search_id);
    let uses_session = true;

    // Fetch the list of tabs
    let response = reqwest::get("http://localhost:9222/json").await?;
    let tabs: Vec<serde_json::Value> = response.json().await?;
    let mut futures = FuturesUnordered::new();

    for tab in tabs {
        let tab_url = tab["url"].as_str().unwrap_or("<no url>").to_string();
        let target_id = tab["id"].as_str().unwrap_or("<no id>").to_string();
        let title = tab["title"].as_str().unwrap_or("<no title>").to_string();
        let page_url = tab["url"].as_str().unwrap_or("<no url>").to_string();

        // Check if the tab_url contains the search_id directly
        if tab_url.contains(&format!("#{}", search_id)) {
            log::debug!("Found tab with bangId {} in URL: {}", search_id, tab_url);
            return Ok(Some((target_id, title, page_url)));
        }

        if is_invalid_url(&page_url) {
            continue;
        }

        log::debug!("Searching tab: {}", tab_url);

        if let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() {
            let ws_url = ws_url.to_string();
            let search_id = search_id.to_string();
            let target_id = target_id.clone();
            let title = title.clone();
            let page_url = page_url.clone();

            futures.push(async move {
                if let Ok((mut socket, _)) = connect_async(&ws_url).await {
                    log::debug!("Connected to WebSocket URL: {}", ws_url);

                    // Try to get window.bangId next
                    let command_id = get_unique_id();
                    let get_bang_id_window = serde_json::json!({
                        "id": command_id,
                        "method": "Runtime.evaluate",
                        "params": {
                            "expression": "window.bangId",
                            "returnByValue": true
                        }
                    });

                    if socket.send(Message::Text(get_bang_id_window.to_string().into())).await.is_ok() {
                        log::debug!("Sent command to get window.bangId with id {}", command_id);

                        // Wait for a response
                        let timeout_duration = Duration::from_secs(5);
                        let timeout_future = tokio::time::sleep(timeout_duration);
                        tokio::pin!(timeout_future);

                        'inner: loop {
                            tokio::select! {
                                _ = &mut timeout_future => {
                                    log::debug!("Timeout while waiting for window.bangId response.");
                                    break 'inner;
                                }
                                Some(Ok(msg)) = socket.next() => {
                                    if let Message::Text(txt) = msg {
                                        log::debug!("Received message: {}", txt);
                                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                                            if json["id"] == command_id {
                                                if let Some(bang_id) = json["result"]["result"]["value"].as_str() {
                                                    if bang_id == search_id {
                                                        log::debug!("Found tab with bangId {}: {}", search_id, tab_url);
                                                        return Some((target_id, title, page_url));
                                                    }
                                                }
                                                break 'inner; // Exit loop after processing the response
                                            }
                                        }
                                    }
                                }
                                else => {
                                    log::debug!("WebSocket stream ended unexpectedly.");
                                    break 'inner;
                                }
                            }
                        }
                    }
                    let command_id = get_unique_id();
                    // If window.bangId is not set, try sessionStorage.getItem('bangId')
                    let get_bang_id_session = serde_json::json!({
                        "id": command_id,
                        "method": "Runtime.evaluate",
                        "params": {
                            "expression": "sessionStorage.getItem('bangId')",
                            "returnByValue": true
                        }
                    });

                    if socket.send(Message::Text(get_bang_id_session.to_string().into())).await.is_ok() {
                        log::debug!("Sent command to get sessionStorage.bangId with id {}", command_id);

                        // Wait for a response
                        let timeout_duration = Duration::from_secs(5);
                        let timeout_future = tokio::time::sleep(timeout_duration);
                        tokio::pin!(timeout_future);

                        'inner_session: loop {
                            tokio::select! {
                                _ = &mut timeout_future => {
                                    log::debug!("Timeout while waiting for sessionStorage.bangId response.");
                                    break 'inner_session;
                                }
                                Some(Ok(msg)) = socket.next() => {
                                    if let Message::Text(txt) = msg {
                                        log::debug!("Received message: {}", txt);
                                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                                            if json["id"] == command_id {
                                                if let Some(bang_id) = json["result"]["result"]["value"].as_str() {
                                                    if bang_id == search_id {
                                                        log::debug!("Found tab with bangId {}: {}", search_id, tab_url);
                                                        return Some((target_id, title, page_url));
                                                    }
                                                }
                                                break 'inner_session; // Exit loop after processing the response
                                            }
                                        }
                                    }
                                }
                                else => {
                                    log::debug!("WebSocket stream ended unexpectedly.");
                                    break 'inner_session;
                                }
                            }
                        }
                    }
                    // let get_bang_id = if !uses_session {
                    //     serde_json::json!({
                    //         "id": command_id,
                    //         "method": "Runtime.evaluate",
                    //         "params": {
                    //             "expression": "window.bangId",
                    //             "returnByValue": true
                    //         }
                    //     })
                    // } else {
                    //     serde_json::json!({
                    //         "id": command_id,
                    //         "method": "Runtime.evaluate",
                    //         "params": {
                    //             "expression": "sessionStorage.getItem('bangId')",
                    //             "returnByValue": true
                    //         }
                    //     })
                    // };

                    // if socket.send(Message::Text(get_bang_id.to_string().into())).await.is_ok() {
                    //     log::debug!(
                    //         "Sent command to get bangId with id {} {:?}",
                    //         command_id,
                    //         get_bang_id
                    //     );

                    //     // Wait for a response
                    //     let timeout_duration = Duration::from_secs(5);
                    //     let timeout_future = tokio::time::sleep(timeout_duration);
                    //     tokio::pin!(timeout_future);

                    //     'outer: loop {
                    //         tokio::select! {
                    //             _ = &mut timeout_future => {
                    //                 log::debug!("Timeout while waiting for bangId response.");
                    //                 break 'outer;
                    //             }
                    //             Some(Ok(msg)) = socket.next() => {
                    //                 if let Message::Text(txt) = msg {
                    //                     log::debug!("Received message: {}", txt);
                    //                     if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                    //                         if json["id"] == command_id {
                    //                             if let Some(bang_id) =
                    //                                 json["result"]["result"]["value"].as_str()
                    //                             {
                    //                                 if bang_id == search_id {
                    //                                     log::debug!(
                    //                                         "Found tab with bangId {}: {}",
                    //                                         search_id,
                    //                                         tab_url
                    //                                     );
                    //                                     return Some((target_id, title, page_url));
                    //                                 }
                    //                             }
                    //                             break; // Exit loop after processing the response
                    //                         }
                    //                     }
                    //                 }
                    //             }
                    //             else => {
                    //                 log::debug!("WebSocket stream ended unexpectedly.");
                    //                 break;
                    //             }
                    //         }
                    //     }
                    // }
                }
                None
            });
        }
    }

    // Process all futures and return the first match
    while let Some(result) = futures.next().await {
        if let Some(tab_info) = result {
            return Ok(Some(tab_info));
        }
    }

    log::debug!("No tab found with bangId = {}", search_id);
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
    let (mut socket, _) = tokio_tungstenite::connect_async(ws_url).await?;

    // Send the Target.activateTarget command
    let activate_command = serde_json::json!({
        "id": get_unique_id(),
        "method": "Target.activateTarget",
        "params": { "targetId": target_id }
    });
    socket
        .send(Message::Text(activate_command.to_string().into()))
        .await?;
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
        let width = monitor.rect.right - monitor.rect.left;
        let height = monitor.rect.bottom - monitor.rect.top;
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
async fn set_tab_title(target_id: &str, new_title: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = connect_async(&socket_url).await?;
    let enable = serde_json::json!({
        "id": 1,
        "method": "Runtime.enable"
    });
    socket
        .send(Message::Text(enable.to_string().into()))
        .await?;
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

    socket
        .send(Message::Text(set_title_command.to_string().into()))
        .await?;
    // Drain until we see our eval response
    let timeout_duration = Duration::from_secs(5); // Set a timeout duration
    let timeout_future = tokio::time::sleep(timeout_duration);
    tokio::pin!(timeout_future);

    'outer: while let Some(Ok(msg)) = tokio::select! {
        _ = &mut timeout_future => {
            log::debug!("Timeout while waiting for title set response.");
            break 'outer;
        }
        msg = socket.next() => msg
    } {
        if let Message::Text(txt) = msg {
            if let Ok(json) = serde_json::from_str::<Value>(&txt) {
                if json["id"].as_i64() == Some(id.try_into().unwrap()) {
                    log::debug!("✅ title set response: {}", txt);
                    break;
                }
            }
        }
    }

    log::debug!("Set tab {} title to: {}", target_id, new_title);

    Ok(())
}

use futures_util::SinkExt;
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

async fn refresh_tab(target_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = connect_async(&socket_url).await?;

    // Send the Page.reload command
    let reload_command = serde_json::json!({
        "id": 1,
        "method": "Page.reload",
        "params": {}
    });

    socket
        .send(Message::Text(reload_command.to_string().into()))
        .await?;
    log::debug!("Sent command to refresh tab with targetId: {}", target_id);

    // Optionally, wait for a response to confirm the reload
    let timeout_duration = Duration::from_secs(5); // Set a timeout duration
    let timeout_future = tokio::time::sleep(timeout_duration);
    tokio::pin!(timeout_future);

    tokio::select! {
        _ = &mut timeout_future => {
            log::debug!("Timeout while waiting for response.");
        }
        Some(Ok(msg)) = socket.next() => {
            if let Message::Text(txt) = msg {
                log::debug!("Received response: {}", txt);
            }
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
    let (mut socket, _) = tokio_tungstenite::connect_async(ws_url).await?;

    let close_command = serde_json::json!({
        "id": get_unique_id(),
        "method": "Target.closeTarget",
        "params": { "targetId": target_id }
    });

    let max_retries = 20;
    let mut attempts = 0;

    while attempts < max_retries {
        if socket
            .send(Message::Text(close_command.to_string().into()))
            .await
            .is_ok()
        {
            log::debug!("Sent command to close tab with targetId: {}", target_id);

            // Wait for a response to confirm the tab was closed with a timeout
            let timeout_duration = Duration::from_secs(5);
            let response = tokio::time::timeout(timeout_duration, socket.next()).await;

            match response {
                Ok(Some(Ok(Message::Text(txt)))) => {
                    log::debug!("Received response: {}", txt);
                    let json: serde_json::Value = serde_json::from_str(&txt)?;
                    if json["id"] == close_command["id"] {
                        log::debug!("Tab with targetId {} closed successfully.", target_id);
                        break;
                    } else {
                        log::debug!("Failed to close tab with targetId: {}", target_id);
                        if let Ok(mut file) = fs::OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open("debugchrome_error.log")
                        {
                            writeln!(file, "Failed to close tab with targetId: {}", target_id).ok();
                        }
                    }
                }
                Ok(Some(_)) => {
                    log::debug!("Unexpected message received while closing tab.");
                }
                Ok(None) => {
                    log::debug!("WebSocket stream ended unexpectedly.");
                }
                Err(_) => {
                    log::debug!("Timeout while waiting for response to close tab.");
                }
            }
        }

        attempts += 1;
        log::debug!(
            "Retrying to close tab with targetId: {} (attempt {}/{})",
            target_id,
            attempts,
            max_retries
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    if attempts == max_retries {
        log::debug!(
            "Failed to close tab with targetId: {} after {} attempts.",
            target_id,
            max_retries
        );
    }

    Ok(())
}

async fn set_bang_id_session(
    target_id: &str,
    bang_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = connect_async(&socket_url).await?;
    // Set the hash for the page to the bangId
    let set_hash = serde_json::json!({
        "id": get_unique_id(),
        "method": "Runtime.evaluate",
        "params": {
            "expression": format!("window.location.hash = '{}';", bang_id),
        }
    });
    log::debug!("set_hash: {:?}", set_hash);
    socket
        .send(Message::Text(set_hash.to_string().into()))
        .await?;
    log::debug!("Set window.location.hash to {}", bang_id);
    // Set the bangId in sessionStorage
    let set_bang_id = serde_json::json!({
        "id": get_unique_id(),
        "method": "Runtime.evaluate",
        "params": {
            // "expression": format!("sessionStorage.setItem('bangId', '{}');", bang_id),
            "expression": format!("window.bangId='{}';", bang_id),
        }
    });
    log::debug!("set_bang_id: {:?}", set_bang_id);
    socket
        .send(Message::Text(set_bang_id.to_string().into()))
        .await?;
    log::debug!("Set window.bangId to {}", bang_id);

    // Verify that the bangId was set
    let verify_bang_id = serde_json::json!({
        "id": 4,
        "method": "Runtime.evaluate",
        "params": {
            // "expression": "sessionStorage.getItem('bangId')",
            "expression": "window.bangId",
        }
    });
    log::debug!("verify_bang_id: {:?}", verify_bang_id);
    socket
        .send(Message::Text(verify_bang_id.to_string().into()))
        .await?;
    log::debug!("Sent command to verify bangId");

    // Wait for the response
    let timeout_duration = Duration::from_secs(5);
    let timeout_future = tokio::time::timeout(timeout_duration, async {
        while let Some(Ok(msg)) = socket.next().await {
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
        Err("Timeout while verifying bangId".into())
    });

    timeout_future.await.unwrap_or_else(|_| {
        log::debug!("Timeout while verifying bangId");
        Err("Timeout while verifying bangId".into())
    })
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
fn get_monitor_bounds() -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();

    unsafe extern "system" fn monitor_enum_proc(
        hmonitor: HMONITOR,
        _: *mut winapi::shared::windef::HDC__,
        lprc_monitor: *mut RECT,
        lparam: isize,
    ) -> i32 {
        let monitors = unsafe { &mut *(lparam as *mut Vec<MonitorInfo>) };

        // Retrieve DPI scaling for the monitor
        let dpi_scaling = get_dpi_for_monitor(hmonitor);

        monitors.push(MonitorInfo {
            hmonitor,
            rect: unsafe { *lprc_monitor },
            dpi_scaling, // Populate DPI scaling
        });
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
    dpi_scaling_enabled: bool,
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

        let monitor = &monitors[monitor_index];
        // let monitor_width = monitor.rect.right - monitor.rect.left;
        // let monitor_height = monitor.rect.bottom - monitor.rect.top;

        log::debug!(
            "Monitor bounds: left={}, top={}, right={}, bottom={}",
            monitor.rect.left,
            monitor.rect.top,
            monitor.rect.right,
            monitor.rect.bottom
        );
        // Adjust bounds relative to the monitor
        let x = monitor.rect.left + x;
        let y = monitor.rect.top + y;
        // let w = w.min(monitor_width);
        // let h = h.min(monitor_height);

        // Apply DPI scaling if enabled
        let (adjusted_x, adjusted_y, adjusted_w, adjusted_h) = if dpi_scaling_enabled {
            adjust_for_dpi(x, y, w, h, monitor.dpi_scaling)
        } else {
            (x, y, w, h)
        };
        if dpi_scaling_enabled {
            log::debug!(
                "Monitor DPI scaling: {}, Adjusted bounds: x={}, y={}, w={}, h={}",
                monitor.dpi_scaling,
                adjusted_x,
                adjusted_y,
                adjusted_w,
                adjusted_h
            );
        }

        Some((adjusted_x, adjusted_y, adjusted_w, adjusted_h))
    }
}

#[cfg(target_os = "windows")]
use winapi::um::shellscalingapi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
#[cfg(target_os = "windows")]
use winapi::um::winnt::HRESULT;

#[cfg(target_os = "windows")]
fn get_dpi_for_monitor(monitor: HMONITOR) -> f32 {
    unsafe {
        let mut dpi_x = 0;
        let mut dpi_y = 0;
        let result: HRESULT = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        if result == 0 {
            dpi_x as f32 / 96.0 // Convert DPI to scaling factor (96 DPI = 100%)
        } else {
            1.0 // Default scaling factor if DPI retrieval fails
        }
    }
}

#[cfg(target_os = "windows")]
fn adjust_for_dpi(x: i32, y: i32, w: i32, h: i32, dpi_scaling: f32) -> (i32, i32, i32, i32) {
    (
        (x as f32 / dpi_scaling).round() as i32,
        (y as f32 / dpi_scaling).round() as i32,
        (w as f32 / dpi_scaling).round() as i32,
        (h as f32 / dpi_scaling).round() as i32,
    )
}

use std::fs::OpenOptions;
use std::panic;

fn setup_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let panic_message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic message".to_string()
        };

        let location = if let Some(location) = info.location() {
            format!(
                "Panic occurred in file '{}:{}'",
                std::fs::canonicalize(location.file())
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| location.file().to_string()),
                location.line()
            )
        } else {
            "Unknown location".to_string()
        };
        log::error!("PANIC: {}\n{}", panic_message, location);

        let log_file_path = if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                exe_dir
                    .join("debugchrome.log")
                    .to_string_lossy()
                    .to_string()
            } else {
                "debugchrome.log".to_string()
            }
        } else {
            "debugchrome.log".to_string()
        };

        // Optionally, display a message box or keep the window open
        #[cfg(target_os = "windows")]
        unsafe {
            use std::ffi::CString;
            use winapi::um::winuser::{MB_ICONERROR, MB_OK, MessageBoxA};

            let message = CString::new(format!(
                "The application encountered an error and needs to close. Check the log for details.\n\nPanic Message: {}\n{}",
                panic_message, &log_file_path.clone()
            ))
            .unwrap();
            let title = CString::new("Application Error").unwrap();
            MessageBoxA(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }));
}

use sysinfo::{Process, System};
use winapi::um::handleapi::CloseHandle;
use winapi::um::processthreadsapi::OpenProcess;
use winapi::um::winnt::PROCESS_QUERY_INFORMATION;
use winapi::um::winuser::{GetWindowThreadProcessId, IsWindowVisible};

fn find_chrome_with_debug_port() -> Option<u32> {
    // Structure to hold the matching process ID
    struct EnumData {
        pid: u32,
        hwnd: Option<winapi::shared::windef::HWND>,
    }

    unsafe extern "system" fn enum_windows_proc(
        hwnd: winapi::shared::windef::HWND,
        lparam: winapi::shared::minwindef::LPARAM,
    ) -> i32 {
        let data = &mut *(lparam as *mut EnumData);

        // Check if the window is visible
        if IsWindowVisible(hwnd) == 0 {
            return 1; // Continue enumeration
        }

        // Get the process ID for the window
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);

        // Check if the process ID matches
        if process_id == data.pid {
            data.hwnd = Some(hwnd);
            return 0; // Stop enumeration
        }

        1 // Continue enumeration
    }

    // Create a system object to get process information
    let mut system = System::new_all();
    system.refresh_all();

    // Iterate over all processes
    for (pid, process) in system.processes() {
        // Check if the process name contains "chrome.exe"
        if process
            .name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains("chrome.exe")
        {
            // Check if the command line contains "--remote-debugging-port"
            if let Some(cmd) = process
                .cmd()
                .join(std::ffi::OsStr::new(" "))
                .to_string_lossy()
                .to_ascii_lowercase()
                .find("--remote-debugging-port")
            {
                println!(
                    "Found Chrome process with PID: {} and command line: {:?}",
                    pid,
                    process.cmd().join(std::ffi::OsStr::new(" "))
                );

                // Check if the process has a main window
                let mut data = EnumData {
                    pid: pid.as_u32(),
                    hwnd: None,
                };
                unsafe {
                    EnumWindows(
                        Some(enum_windows_proc),
                        &mut data as *mut _ as winapi::shared::minwindef::LPARAM,
                    );
                }

                if let Some(hwnd) = data.hwnd {
                    println!("Found Chrome window with HWND: {:?}", hwnd);
                    return Some(pid.as_u32());
                }
            }
        }
    }

    None
}

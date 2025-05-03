use std::{env, fs, io, thread};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use futures_util::stream::FuturesUnordered;
use reqwest::Client;
use serde_json::Value;
use tokio::time::timeout;
use tungstenite::Message;
use base64::Engine;
use std::sync::atomic::{AtomicUsize, Ordering};
use rayon::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;

use futures_util::TryFutureExt;

// Global atomic counter for unique IDs
static COMMAND_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn get_unique_id() -> usize {
    COMMAND_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}
#[cfg(target_os = "windows")]
fn bring_chrome_to_front_and_resize_with_powershell(bounds: Option<(i32, i32, i32, i32)>) {
    let ps_script = if let Some((x, y, w, h)) = bounds {
        // PowerShell script to move and resize the window
        format!(
            r#"
            $chrome = Get-Process chrome | Where-Object {{ $_.MainWindowHandle -ne 0 -and $_.Path -like '*chrome.exe' }} | Select-Object -First 1
            if ($chrome) {{
                $sig = '[DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);'
                $sig += '[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);'
                $sig += '[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);'
                Add-Type -MemberDefinition $sig -Name NativeMethods -Namespace WinAPI | Out-Null
                $hWnd = $chrome.MainWindowHandle
                [WinAPI.NativeMethods]::MoveWindow($hWnd, {x}, {y}, {w}, {h}, $true) | Out-Null
                [WinAPI.NativeMethods]::SetForegroundWindow($hWnd) | Out-Null
                [WinAPI.NativeMethods]::ShowWindowAsync($hWnd, 9) | Out-Null
            }}
            "#,
            x = x,
            y = y,
            w = w,
            h = h
        )
    } else {
        // PowerShell script to only bring the window to the front
        r#"
        $chrome = Get-Process chrome | Where-Object { $_.MainWindowHandle -ne 0 -and $_.Path -like '*chrome.exe' } | Select-Object -First 1
        if ($chrome) {
            $sig = '[DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);'
            $sig += '[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);'
            Add-Type -MemberDefinition $sig -Name NativeMethods -Namespace WinAPI | Out-Null
            $hWnd = $chrome.MainWindowHandle
            [WinAPI.NativeMethods]::SetForegroundWindow($hWnd) | Out-Null
            [WinAPI.NativeMethods]::ShowWindowAsync($hWnd, 9) | Out-Null
        }
        "#.to_string()
    };

    let _ = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(ps_script)
        .status();
}

#[tokio::main]
async fn main() -> std::io::Result<()> {

    let args: Vec<String> = env::args().collect();
        // Set the current working directory to the directory of the executing binary
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                std::env::set_current_dir(exe_dir)?;
                println!("Working directory set to: {:?}", exe_dir);
            }
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
            eprintln!("Failed to register debugchrome protocol: {}", e);
            println!("Try running this program in an elevated command prompt (Run as Administrator).");
            println!("or double click the reg file.");
        } else {
            println!("Registered debugchrome protocol successfully.");
        }
        return Ok(());
    }

    if args.len() > 2 && args[1] == "--search" {
        let search_id = &args[2];
        if let Err(e) = search_tabs_for_bang_id(search_id).await {
            eprintln!("Failed to search tabs: {}", e);
        }
        return Ok(());
    }

    if args.len() > 1 {
        let raw_url = &args[1];
        let translated = raw_url.replacen("debugchrome:", "", 1);
        let user_data_dir = std::env::temp_dir().join("chromedev");

            // Check if the CDP server is running
    if (!is_cdp_server_running()) {
        println!("CDP server is not running. Preparing Chrome profile and launching Chrome...");

        // Prepare Chrome profile
        let user_data_dir = prepare_chrome_profile()?;
        println!("User data cloned to: {}", user_data_dir.display());

        // Launch Chrome
        launch_chrome(&user_data_dir)?;
        println!("Chrome launched successfully. Waiting for the CDP server to start...");
        std::thread::sleep(Duration::from_secs(5)); // Wait for the server to start
    } else {
        println!("CDP server is already running.");
    }
    

    // Encode the URL
    let encoded_url = encode_url(&translated).map_err(|e| {
        eprintln!("Failed to encode URL: {}", e);
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    })?;

        // Check if the bangId is already open
        let parsed_url = url::Url::parse(&encoded_url).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        if let Some(bang_id) = parsed_url.query_pairs().find(|(k, _)| k == "!id").map(|(_, v)| v.to_string()) {
            if let Some((target_id,title, url)) = search_tabs_for_bang_id(&bang_id).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)).await? {
                println!("Tab with bangId {} title {} is already open: {}", bang_id, title, target_id);
            
                // Activate the tab
                if let Err(e) = activate_tab(&target_id) {
                    eprintln!("Failed to activate tab: {}", e);
                }
                let (_, (x, y, w, h)) = parse_screen_bounds(&parsed_url);
                let bounds = if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                    Some((x, y, w, h))
                } else {
                    None
                };
                bring_chrome_to_front_and_resize_with_powershell(bounds);
                // if let Err(e) = set_tab_title(&target_id, &target_id){
                //     eprintln!("Failed to set tab title: {}", e);
                // }
                // if let Some(hwnd) = find_chrome_hwnd_by_title(&target_id) {
                //     bring_hwnd_to_front(hwnd);
                // } else {
                //     eprintln!("Failed to find Chrome window with title '{}'.",&target_id);
                // }
                // set_tab_title(&target_id, &title).ok();
                refresh_tab(&target_id).ok();
                return Ok(());
            }
        }

        if let Ok((target_id, bounds)) = open_tab_via_devtools_and_return_id(&translated).await {
            if let Some((x, y, w, h)) = bounds {
                set_window_bounds(&target_id, x, y, w, h).ok();
                bring_chrome_to_front_and_resize_with_powershell(bounds);
            }
            // if let Some(hwnd) = find_chrome_hwnd_by_title(&target_id) {
            //     bring_hwnd_to_front(hwnd);
            // } else {
            //     eprintln!("Failed to find Chrome window with title '{}'.",&target_id);
            // }
            if let Err(e) = take_screenshot(&target_id) {
                eprintln!("Failed to take screenshot: {}", e);
                std::thread::sleep(std::time::Duration::from_secs(3)); // Ensure sleep even on error
                return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e)));
            }

            // Call set_bang_id to set the bangId in the tab
            println!("Setting bangId in the tab...{}", &translated);
            if let Err(e) = set_bang_id(&target_id, &translated) {
                eprintln!("Failed to set bangId: {}", e);
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

        println!("Requested debug Chrome with URL: {}", translated);
    } else {
        println!("Usage:");
        println!("  debugchrome.exe \"debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123\"");
        println!("  debugchrome.exe --search 123");
        println!("  debugchrome.exe --register");
    }

    Ok(())
}
fn parse_screen_bounds(parsed: &url::Url) -> ((i32, i32), (Option<i32>, Option<i32>, Option<i32>, Option<i32>)) {
    // Get screen dimensions dynamically
    let (screen_width, screen_height) = get_screen_resolution();
    // Parse !x, !y, !w, and !h
    let x = parsed.query_pairs().find(|(k, _)| k == "!x").and_then(|(_, v)| parse_dimension(&v, screen_width));
    let y = parsed.query_pairs().find(|(k, _)| k == "!y").and_then(|(_, v)| parse_dimension(&v, screen_height));
    let w = parsed.query_pairs().find(|(k, _)| k == "!w").and_then(|(_, v)| parse_dimension(&v, screen_width));
    let h = parsed.query_pairs().find(|(k, _)| k == "!h").and_then(|(_, v)| parse_dimension(&v, screen_height));

    ((screen_width, screen_height), (x, y, w, h))
}

async fn open_tab_via_devtools_and_return_id(url: &str) -> Result<(String, Option<(i32, i32, i32, i32)>), Box<dyn std::error::Error>> {
    let parsed = url::Url::parse(url)?;
    let clean_url = format!("{}://{}{}", parsed.scheme(), parsed.host_str().unwrap_or(""), parsed.path());
    let (_, (x, y, w, h)) = parse_screen_bounds(&parsed);
    let response = reqwest::get("http://localhost:9222/json/version").await?;
    let version: serde_json::Value = response.json().await?;
    let ws_url = version["webSocketDebuggerUrl"].as_str().ok_or("No WebSocket URL")?;
    let (socket, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let mut socket = socket;

    let msg = serde_json::json!({
        "id": 1,
        "method": "Target.createTarget",
        "params": { "url": clean_url }
    });

    socket.send(tungstenite::Message::Text(msg.to_string().into())).await?;

    let timeout = std::time::Duration::from_secs(5); // Define a timeout duration
    match tokio::time::timeout(timeout, socket.next()).await {
        Ok(Some(msg)) => {
            if let Ok(Message::Text(txt)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if let Some(target_id) = json["result"]["targetId"].as_str() {
                        return Ok((target_id.to_string(), Some((x.unwrap_or(0), y.unwrap_or(0), w.unwrap_or(1024), h.unwrap_or(768)))));
                    }
                }
            }
        }
        Ok(Some(Err(e))) => {
            eprintln!("Error reading from WebSocket: {}", e);
        }
        Ok(None) => {
            eprintln!("WebSocket stream ended unexpectedly.");
        }
        Err(_) => {
            eprintln!("Timeout while reading from WebSocket.");
        }
    }

    Err("Failed to get targetId".into())
}

fn set_window_bounds(target_id: &str, x: i32, y: i32, w: i32, h: i32) -> Result<(), Box<dyn std::error::Error>> {
    let window_id_resp: serde_json::Value = reqwest::blocking::get("http://localhost:9222/json")?
        .json()?;

    let version: serde_json::Value = reqwest::blocking::get("http://localhost:9222/json/version")?.json()?;
    let ws_url = version["webSocketDebuggerUrl"].as_str().ok_or("No WebSocket URL")?;
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
    println!("Sending captureScreenshot command: {}", capture);
    socket.send(tungstenite::Message::Text(capture.to_string().into()))?;

    println!("Current directory: {:?}", std::env::current_dir()?);
    while let Ok(msg) = socket.read() {
        if let tungstenite::Message::Text(txt) = msg {
            let json: serde_json::Value = serde_json::from_str(&txt)?;
            if let Some(data) = json["result"]["data"].as_str() {
                let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
                std::fs::write("screenshot.png", bytes)?;
                println!("Screenshot saved to screenshot.png");
                break;
            }
        }
    }
    //std::thread::sleep(std::time::Duration::from_secs(30));
    Ok(())
}

fn set_bang_id(target_id: &str, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = url::Url::parse(url)?;
    if let Some(bang_id) = parsed.query_pairs().find(|(k, _)| k == "!id").map(|(_, v)| v.to_string()) {
        let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
        let (mut socket, _) = tungstenite::connect(&socket_url)?;

        // Set the bangId
        let set_bang_id = serde_json::json!({
            "id": 3,
            "method": "Runtime.evaluate",
            "params": {
                "expression": format!("window.bangId = '{}';", bang_id),
            }
        });
        socket.send(Message::Text(set_bang_id.to_string().into()))?;
        println!("Set window.bangId to {}", bang_id);

        // Verify that the bangId was set
        let verify_bang_id = serde_json::json!({
            "id": 4,
            "method": "Runtime.evaluate",
            "params": {
                "expression": "window.bangId",
            }
        });
        socket.send(Message::Text(verify_bang_id.to_string().into()))?;
        println!("Sent command to verify bangId");

        // Wait for the response
        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);
        while start_time.elapsed() < timeout {
            if let Ok(msg) = socket.read() {
                if let Message::Text(txt) = msg {
                    println!("Received message: {}", txt);
                    let json: serde_json::Value = serde_json::from_str(&txt)?;
                    if json["id"] == 4 {
                        if let Some(verified_bang_id) = json["result"]["result"]["value"].as_str() {
                            if verified_bang_id == bang_id {
                                println!("Successfully verified bangId: {}", verified_bang_id);
                                return Ok(());
                            } else {
                                eprintln!("Mismatch: Expected {}, but got {}", bang_id, verified_bang_id);
                                return Err("Failed to verify bangId".into());
                            }
                        }
                    }
                }
            }
        }

        eprintln!("Timeout while verifying bangId");
        return Err("Timeout while verifying bangId".into());
    }
    Ok(())
}
// pub async fn search_tabs_for_bang_id(
//     search_id: &str,
// ) -> Result<Option<(String, String, String)>, Box<dyn std::error::Error + Send + Sync>> {
//     // Fetch tabs list
//     let client = Client::new();
//     let tabs: Vec<Value> = client
//         .get("http://localhost:9222/json")
//         .send()
//         .await?
//         .json()
//         .await?;

//     // Create a channel for message dispatch
//     let (sender, mut receiver) = mpsc::channel(100);

//     // Spawn a centralized WebSocket reader
//     let ws_url = "ws://localhost:9222/devtools/browser/<browser-id>"; // Replace with actual WebSocket URL
//     let (socket, _) = connect_async(ws_url).await?;
//     tokio::spawn(centralized_websocket_reader(socket, sender));

//     // Process tabs concurrently
//     let mut tasks = FuturesUnordered::new();
//     for tab in tabs {
//         let search_id = search_id.to_string();
//         let (sender_clone, receiver_clone) = mpsc::channel(100);
//         let mut receiver = receiver_clone;

//         tasks.push(tokio::spawn(async move {
//             let target_id = tab["id"].as_str().unwrap_or("<no id>").to_string();
//             let title = tab["title"].as_str().unwrap_or("<no title>").to_string();
//             let page_url = tab["url"].as_str().unwrap_or("<no url>").to_string();

//             if is_invalid_url(&page_url) {
//                 return None;
//             }

//             if let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() {
//                 let command_id = get_unique_id();
//                 if let Ok(Some(bang_id)) = process_tab(ws_url, command_id, &search_id, &mut receiver).await {
//                     return Some((target_id, title, page_url));
//                 }
//             }

//             None
//         }));
//     }

//     // Wait for all tasks to complete
//     while let Some(result) = tasks.next().await {
//         if let Some(found) = result.unwrap_or(None) {
//             return Ok(Some(found));
//         }
//     }

//     Ok(None)
// }


async fn search_tabs_for_bang_id(search_id: &str) -> Result<Option<(String, String, String)>, Box<dyn std::error::Error + Send + Sync>> {
    println!("Searching for bangId = {}", search_id);

    // Fetch the list of tabs
    let response = reqwest::get("http://localhost:9222/json").await?;
    let tabs: Vec<serde_json::Value> = response.json().await?;
    let results = Arc::new(std::sync::Mutex::new(None)); // Shared result storage

    // Process tabs in parallel using rayon
    tabs.par_iter().for_each(|tab| {
        let tab_url = tab["url"].as_str().unwrap_or("<no url>");
            let target_id = tab["id"].as_str().unwrap_or("<no id>").to_string();
            let title = tab["title"].as_str().unwrap_or("<no title>").to_string();
            let page_url = tab["url"].as_str().unwrap_or("<no url>").to_string();

            if is_invalid_url(&page_url) {
                return;
            }


        println!("Searching tab: {}", tab_url);
        if let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() {
            let results = Arc::clone(&results);

            // Use a timeout for the WebSocket operation
            let start_time = std::time::Instant::now();
            let timeout = Duration::from_secs(5);

            if let Ok((mut socket, _)) = tungstenite::connect(ws_url) {
                println!("Connected to WebSocket URL: {}", ws_url);

                // Generate a unique ID for this command
                let command_id = get_unique_id();

                // Send the Runtime.evaluate command to get window.bangId
                let get_bang_id = serde_json::json!({
                    "id": command_id,
                    "method": "Runtime.evaluate",
                    "params": {
                        "expression": "window.bangId"
                    }
                });
                if socket.send(Message::Text(get_bang_id.to_string().into())).is_ok() {
                    println!("Sent command to get bangId with id {}", command_id);

                    // Wait for a response with a timeout
                    while start_time.elapsed() < timeout {
                        if let Ok(msg) = socket.read() {
                            if let Message::Text(txt) = msg {
                                println!("Received message: {}", txt);
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                                    if json["id"] == command_id {
                                        if let Some(bang_id) = json["result"]["result"]["value"].as_str() {
                                            if bang_id == search_id {
                                                println!(
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
                                                        eprintln!("Failed to acquire lock on results");
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

            println!(
                "Timeout or no matching response while searching tab with WebSocket URL: {}",
                ws_url
            );
        }
    });

    // Check if a result was found
    if let Some(url) = &*results.lock().unwrap() {
        println!("Found tab with bangId {}: {:?}", search_id, url);
    } else {
        println!("No tab found with bangId = {}", search_id);
    }

    Ok(None)
}

async fn process_tab(
    ws_url: &str,
    command_id: usize,
    search_id: &str,
    receiver: &mut mpsc::Receiver<serde_json::Value>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let (mut socket, _) = connect_async(ws_url).await?;
    let request_text = serde_json::json!({
        "id": command_id,
        "method": "Runtime.evaluate",
        "params": { "expression": "window.bangId" }
    });

    socket.send(Message::Text(request_text.to_string().into())).await?;
    println!("Sent command to get bangId with id {}", command_id);

    // Wait for the response
    while let Some(json) = receiver.recv().await {
        if json["id"] == command_id {
            if let Some(bang_id) = json["result"]["result"]["value"].as_str() {
                if bang_id == search_id {
                    return Ok(Some(bang_id.to_string()));
                }
            }
            break;
        }
    }

    Ok(None)
}

fn activate_tab(target_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Fetch the WebSocket debugger URL
    let version: serde_json::Value = reqwest::blocking::get("http://localhost:9222/json/version")?.json()?;
    let ws_url = version["webSocketDebuggerUrl"].as_str().ok_or("No WebSocket URL")?;
    
    // Connect to the WebSocket
    let (mut socket, _) = tungstenite::connect(ws_url)?;
    
    // Send the Target.activateTarget command
    let activate_command = serde_json::json!({
        "id": get_unique_id(),
        "method": "Target.activateTarget",
        "params": { "targetId": target_id }
    });
    socket.send(Message::Text(activate_command.to_string().into()))?;
    println!("Activated tab with targetId: {}", target_id);
    
    Ok(())
}

fn is_invalid_url(url: &str) -> bool {
    // List of URL prefixes or patterns to exclude
    let invalid_prefixes = [
        "ws://",                  // WebSocket URLs
        "chrome-extension://",    // Chrome extensions
        "chrome://",              // Internal Chrome pages
        "chrome-untrusted://",              // Internal Chrome pages
        "about:",                 // About pages
        "file://",                // Local file URLs
        "data:",                  // Data URLs
        "javascript:",            // JavaScript URLs
    ];

    // Check if the URL starts with any of the invalid prefixes
    invalid_prefixes.iter().any(|prefix| url.starts_with(prefix))
}

use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

fn get_screen_resolution() -> (i32, i32) {
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    (width, height)
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

use winapi::um::winuser::{EnumWindows, GetWindowTextA, GetClassNameA, IsWindowVisible};
use winapi::shared::windef::HWND;
use std::ffi::CString;
use std::ptr;

fn find_chrome_hwnd_by_title(title: &str) -> Option<HWND> {
    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: isize) -> i32 {
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
            println!("Window title: {}", window_title);
            if window_title.contains(title_ptr) { //&& IsWindowVisible(hwnd) != 0 {
                *hwnd_ptr = hwnd;
                return 0; // Stop enumeration
            }
        }
        1 // Continue enumeration
    }

    let mut hwnd: HWND = ptr::null_mut();
    let mut data = (title.to_string(), hwnd);
    unsafe {
        EnumWindows(Some(enum_windows_proc), &mut data as *mut _ as isize);
    }

    if data.1.is_null() {
        None
    } else {
        Some(data.1)
    }
}

fn set_tab_title(target_id: &str, new_title: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_url = format!("ws://localhost:9222/devtools/page/{}", target_id);
    let (mut socket, _) = tungstenite::connect(&socket_url)?;
    let enable = serde_json::json!({
        "id": 1,
        "method": "Runtime.enable"
    });
    socket.write_message(Message::Text(enable.to_string().into()))?;
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
                            println!("✅ title set response: {}", txt);
                        }
                    }
                }
                _ => {}
            }

   println!("{:?}", socket.read()?); // Read the response
    println!("Set tab {} title to: {}", target_id, new_title);

    Ok(())
}

use winapi::um::winuser::{SetForegroundWindow, ShowWindow, SW_RESTORE};
use tokio_tungstenite::connect_async;
use futures_util::{StreamExt, SinkExt};

fn bring_hwnd_to_front(hwnd: HWND) {
    if hwnd.is_null() {
        eprintln!("Invalid HWND: Cannot bring to front.");
        return;
    }

    unsafe {
        // Restore the window if it is minimized
        ShowWindow(hwnd, SW_RESTORE);
        // Bring the window to the foreground
        SetForegroundWindow(hwnd);
    }
}

fn is_cdp_server_running() -> bool {
    match reqwest::blocking::get("http://localhost:9222/json") {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}


fn prepare_chrome_profile() -> io::Result<std::path::PathBuf> {
    let chrome_user_data = dirs::data_local_dir()
        .expect("%LOCALAPPDATA% not found")
        .join("Google\\Chrome\\User Data");

    let source_default = chrome_user_data.join("Default");
    let source_local_state = chrome_user_data.join("Local State");
    let source_sessions = source_default.join("Sessions");

    let temp_root = env::temp_dir().join("chromedev");
    let temp_default = temp_root.join("Default");
    let temp_sessions = temp_default.join("Sessions");

    let _ = Command::new("taskkill").args(["/F", "/IM", "chrome.exe"]).output();
    thread::sleep(Duration::from_secs(1));

    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_default)?;
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

    Ok(temp_root)
}

fn launch_chrome(user_data_dir: &Path) -> io::Result<()> {
    Command::new("cmd")
        .args([
            "/C", "start", "chrome.exe",
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
    println!("Sent command to refresh tab with targetId: {}", target_id);

    // Optionally, wait for a response to confirm the reload
    if let Ok(msg) = socket.read() {
        if let Message::Text(txt) = msg {
            println!("Received response: {}", txt);
        }
    }

    Ok(())
}

async fn centralized_websocket_reader(
    mut socket: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    sender: mpsc::Sender<serde_json::Value>,
) {
    while let Some(msg) = socket.next().await {
        match msg {
            Ok(tungstenite::Message::Text(txt)) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if sender.send(json).await.is_err() {
                        eprintln!("Failed to send message to channel");
                        break;
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("WebSocket read error: {}", e);
                break;
            }
        }
    }
}
fn encode_url(raw_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let parsed_url = url::Url::parse(raw_url)?;
    let mut encoded_url = parsed_url.clone();

    // Rebuild the query string with percent-encoded keys
    {
        let mut query_pairs = encoded_url.query_pairs_mut();
        query_pairs.clear(); // Clear existing query pairs

        for (key, value) in parsed_url.query_pairs() {
            let encoded_key = url::form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>();
            let encoded_value = url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>();
            query_pairs.append_pair(&encoded_key, &encoded_value);
        }
    }

    Ok(encoded_url.to_string())
}
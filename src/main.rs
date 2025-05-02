use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tungstenite::connect;
use tungstenite::Message;
use base64::Engine;
use std::sync::atomic::{AtomicUsize, Ordering};
use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Global atomic counter for unique IDs
static COMMAND_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn get_unique_id() -> usize {
    COMMAND_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn main() -> std::io::Result<()> {
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
        if let Err(e) = search_tabs_for_bang_id(search_id) {
            eprintln!("Failed to search tabs: {}", e);
        }
        return Ok(());
    }

    if args.len() > 1 {
        let raw_url = &args[1];
        let translated = raw_url.replacen("debugchrome:", "", 1);
        let user_data_dir = std::env::temp_dir().join("chromedev");

        if let Ok((target_id, bounds)) = open_tab_via_devtools_and_return_id(&translated) {
            if let Some((x, y, w, h)) = bounds {
                set_window_bounds(&target_id, x, y, w, h).ok();
            }
            if let Err(e) = take_screenshot(&target_id) {
                eprintln!("Failed to take screenshot: {}", e);
                std::thread::sleep(std::time::Duration::from_secs(3)); // Ensure sleep even on error
                return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e)));
            }

            // Call set_bang_id to set the bangId in the tab
            println!("Setting bangId in the tab...{}",&translated);
            if let Err(e) = set_bang_id(&target_id, &translated) {
                eprintln!("Failed to set bangId: {}", e);
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
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
        println!("  debugchrome_launcher.exe debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123");
        println!("  debugchrome_launcher.exe --search 123");
        println!("  debugchrome_launcher.exe --register");
    }

    Ok(())
}

fn open_tab_via_devtools_and_return_id(url: &str) -> Result<(String, Option<(i32, i32, i32, i32)>), Box<dyn std::error::Error>> {
    let parsed = url::Url::parse(url)?;
    let clean_url = format!("{}://{}{}", parsed.scheme(), parsed.host_str().unwrap_or(""), parsed.path());

    let x = parsed.query_pairs().find(|(k, _)| k == "x").and_then(|(_, v)| v.parse().ok());
    let y = parsed.query_pairs().find(|(k, _)| k == "y").and_then(|(_, v)| v.parse().ok());
    let w = parsed.query_pairs().find(|(k, _)| k == "w").and_then(|(_, v)| v.parse().ok());
    let h = parsed.query_pairs().find(|(k, _)| k == "h").and_then(|(_, v)| v.parse().ok());

    let version: serde_json::Value = reqwest::blocking::get("http://localhost:9222/json/version")?.json()?;
    let ws_url = version["webSocketDebuggerUrl"].as_str().ok_or("No WebSocket URL")?;
    let (mut socket, _) = tungstenite::connect(ws_url)?;

    let msg = serde_json::json!({
        "id": 1,
        "method": "Target.createTarget",
        "params": { "url": clean_url }
    });

    socket.send(tungstenite::Message::Text(msg.to_string().into()))?;

    if let Ok(tungstenite::Message::Text(resp)) = socket.read() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
            if let Some(target_id) = json["result"]["targetId"].as_str() {
                return Ok((target_id.to_string(), Some((x.unwrap_or(0), y.unwrap_or(0), w.unwrap_or(1024), h.unwrap_or(768)))));
            }
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
    while let Ok(msg) = socket.read_message() {
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
    std::thread::sleep(std::time::Duration::from_secs(30));
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

fn search_tabs_for_bang_id(search_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Searching for bangId = {}", search_id);

    // Fetch the list of tabs
    let tabs: Vec<serde_json::Value> = reqwest::blocking::get("http://localhost:9222/json")?.json()?;
    let results = Arc::new(Mutex::new(None)); // Shared result storage

    // Process tabs in parallel using rayon
    tabs.par_iter().for_each(|tab| {
        let tab_url = tab["url"].as_str().unwrap_or("<no url>");
        if tab_url.starts_with("ws://") || tab_url.starts_with("chrome-extension://")
            || tab_url.starts_with("chrome://")
            || tab_url.starts_with("file://") 
            || tab_url.starts_with("about:") 
            || tab_url.starts_with("data:") 
            || tab_url.starts_with("view-source:")
            || tab_url.starts_with("devtools://")
            || tab_url.starts_with("chrome-devtools://") 
            || tab_url.starts_with("chrome-untrusted://") {


            println!("Skipping tab with URL: {}", tab_url);
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
                                                let mut results = results.lock().unwrap();
                                                *results = Some(tab_url.to_string());
                                                return;
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
    let results = results.lock().unwrap();
    if let Some(url) = &*results {
        println!("Found tab with bangId {}: {}", search_id, url);
    } else {
        println!("No tab found with bangId = {}", search_id);
    }

    Ok(())
}
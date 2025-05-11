use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;
use cef::{args::Args, rc::*, sandbox_info::SandboxInfo};
use cef::ImplCommandLine;

mod cef_browser;
use cef_browser::CefBrowser;
use cef::CefString;
use cef::*;

pub fn my_initialize(
    args: Option<&MainArgs>,
    settings: Option<&Settings>,
    application: Option<&mut impl ImplApp>,
    windows_sandbox_info: *mut u8,
) -> ::std::os::raw::c_int {
    unsafe {
        let (arg_args, arg_settings, arg_application, arg_windows_sandbox_info) =
            (args, settings, application, windows_sandbox_info);
        let arg_args = arg_args.cloned().map(|arg| arg.into());
        let arg_args = arg_args
            .as_ref()
            .map(std::ptr::from_ref)
            .unwrap_or(std::ptr::null());
        let arg_settings = arg_settings.cloned().map(|arg| arg.into());
        let arg_settings = arg_settings
            .as_ref()
            .map(std::ptr::from_ref)
            .unwrap_or(std::ptr::null());
        let arg_application = arg_application
            .map(|arg| {
                arg.add_ref();
                ImplApp::get_raw(arg)
            })
            .unwrap_or(std::ptr::null_mut());
        let arg_windows_sandbox_info = arg_windows_sandbox_info.cast();
        let result = cef_dll_sys::cef_initialize(
            arg_args,
            arg_settings,
            arg_application,
            arg_windows_sandbox_info,
        );
        result.wrap_result()
    }
}

#[tokio::main]
async fn main() {
    println!("DebugChrome is running...");
    let v = api_hash(sys::CEF_API_VERSION_LAST, 0);
    println!("DebugChrome API hash: {:#x}", v as usize);
    let args = Args::new();
    let cmd = args.as_cmd_line().unwrap();
    let c: CefStringUserfreeUtf16=cmd.command_line_string();
    let p = CefString::from(&c);
    println!("launch process {p}");

    let sandbox = SandboxInfo::new();
    let switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;
    if is_browser_process {
println!("Browser process detected");
    }

    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
        assert!(loader.load());
        loader
    };

    let shared_url = Arc::new(std::sync::Mutex::new(Some("https://openai.com".to_string()))); // Shared state for the URL

    let window = Arc::new(std::sync::Mutex::new(None));
    let mut app = CefBrowser::new(window.clone(), shared_url.clone());
    let ret = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        sandbox.as_mut_ptr(),
    );

    if is_browser_process {
    let mut settings = Settings::default();
    settings.remote_debugging_port = find_available_port(9222).unwrap_or(9222) as i32;
        println!("launch browser process {}", settings.remote_debugging_port);

    fn find_available_port(start_port: u16) -> Option<u16> {

        (start_port..65535).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
    }
    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            sandbox.as_mut_ptr()
        ),
        1
    );

    loop {
        println!("Running...");
        cef::run_message_loop();
        std::thread::sleep(Duration::from_secs(1));
    }

    cef::shutdown();
    let exit_code = get_exit_code();
    std::process::exit(exit_code);
        assert!(ret == -1, "cannot execute browser process");
    } else {

        let process_type = CefString::from(&cmd.switch_value(Some(&switch)));
        println!("launch process {process_type}");
        assert!(ret >= 0, "cannot execute non-browser process");
        // non-browser process does not initialize cef
        return;
    }

}
use cef::{args::Args, rc::*, sandbox_info::SandboxInfo, *};
use cef_dll_sys::cef_event_flags_t as KeyModifiers; // Import KeyModifiers
use tokio::sync::mpsc::{self, Sender, Receiver}; // Import Sender and Receiver
use tokio::{task, time::{sleep, Duration}};
use tokio_tungstenite::connect_async;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;

const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2; // Define the constant manually based on CEF documentation
use std::sync::{Arc, Mutex};
use cef_dll_sys::cef_key_event_t as KeyCode; // Use the correct type for KeyCode
use cef_dll_sys::{cef_errorcode_t as Errorcode, cef_errorcode_t}; // Ensure cef_errorcode_t is imported
use std::env;

pub struct CefBrowser {
    object: *mut RcImpl<cef_dll_sys::_cef_app_t, Self>,
    window: Arc<Mutex<Option<Window>>>,
    shared_url: Arc<Mutex<Option<String>>>, // Shared state for the URL
}

impl CefBrowser {
    pub fn new(window: Arc<Mutex<Option<Window>>>, shared_url: Arc<Mutex<Option<String>>>) -> App {
        App::new(Self {
            object: std::ptr::null_mut(),
            window,
            shared_url,
        })
    }

    pub fn launch(&mut self, url: &str) {
        let args = Args::new();
        let cmd = args.as_cmd_line().unwrap();
        cmd.append_argument(Some(&"--disable-gpu".into())); // Disable GPU acceleration
        cmd.append_argument(Some(&"--disable-software-rasterizer".into())); // Disable software rasterizer
        cmd.append_argument(Some(&"--log-severity=verbose".into()));
        cmd.append_argument(Some(&"--remote-debugging-port=9222".into()));

                
        cmd.append_argument(Some(&CefString::from(format!("--url={}", url).as_str()))); // Pass the URL as an argument
        let sandbox = SandboxInfo::new();

            let switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;

    let window: Arc<Mutex<Option<Window>>> = Arc::new(Mutex::new(None));
    // let mut app = CefBrowser::new();


        let ret = execute_process(
            Some(args.as_main_args()),
            Some(self),
            sandbox.as_mut_ptr(),
        );

        if ret >= 0 {
            return; // Subprocess
        }

        let settings = Settings::default();
        assert_eq!(
            initialize(
                Some(args.as_main_args()),
                Some(&settings),
                Some(self),
                sandbox.as_mut_ptr()
            ),
             1,
            "Failed to initialize CEF"
        );


        run_message_loop();
        // self.create_browser_window(url);

        shutdown();
    }

    fn create_browser_window(&self, url: &str) {
        let mut client = DemoClient::new();
        let mut window_info = WindowInfo::default();
        let settings = BrowserSettings::default();
        let browser = browser_host_create_browser(
            Some(&mut window_info),
            Some(&mut client),
            Some(&CefString::from(url)),
            Some(&settings),
            None::<&mut DictionaryValue>,      // extra_info
            None::<&mut RequestContext>,       // request_context
        );
        // repeat once per window; each will appear under http://127.0.0.1:9222/json

        let (tab_switch_tx, tab_switch_rx) = mpsc::channel(10); // Create a channel for tab-switching events
        let tab_switch_rx = Arc::new(Mutex::new(tab_switch_rx)); // Wrap receiver in Arc<Mutex<>>
        let mut delegate = DemoWindowDelegate::new(tab_switch_rx);
        if let Ok(mut window) = self.window.lock() {
            *window = Some(
                window_create_top_level(Some(&mut delegate)).expect("Failed to create window"),
            );
        }
    }
}

impl WrapApp for CefBrowser {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::_cef_app_t, Self>) {
        self.object = object;
    }
}

impl Clone for CefBrowser {
    fn clone(&self) -> Self {
        let object = if self.object.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe {
                let rc_impl = &mut *self.object;
                rc_impl.interface.add_ref();
                self.object
            }
        };
        let window = self.window.clone();
        let shared_url = self.shared_url.clone();

        Self { object, window, shared_url }
    }
}

impl Rc for CefBrowser {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        if self.object.is_null() {
            panic!("Null pointer dereference: `object` is null in `as_base`");
        }
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl ImplApp for CefBrowser {
    fn get_raw(&self) -> *mut cef_dll_sys::_cef_app_t {
        self.object.cast()
    }

    fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
        Some(DemoBrowserProcessHandler::new(self.window.clone(), self.shared_url.clone()))
    }
}

struct DemoBrowserProcessHandler {
    object: *mut RcImpl<cef_dll_sys::cef_browser_process_handler_t, Self>,
    window: Arc<Mutex<Option<Window>>>,
    shared_url: Arc<Mutex<Option<String>>>, // Shared state for the URL
}

impl DemoBrowserProcessHandler {
    fn new(
        window: Arc<Mutex<Option<Window>>>,
        shared_url: Arc<Mutex<Option<String>>>,
    ) -> BrowserProcessHandler {
        BrowserProcessHandler::new(Self {
            object: std::ptr::null_mut(),
            window,
            shared_url,
        })
    }
}

impl Rc for DemoBrowserProcessHandler {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapBrowserProcessHandler for DemoBrowserProcessHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::cef_browser_process_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for DemoBrowserProcessHandler {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        let window = self.window.clone();
        let shared_url = self.shared_url.clone();

        Self { object, window, shared_url }
    }
}

impl ImplBrowserProcessHandler for DemoBrowserProcessHandler {
    fn get_raw(&self) -> *mut cef_dll_sys::_cef_browser_process_handler_t {
        self.object.cast()
    }

    fn on_context_initialized(&self) {
        println!("CEF context initialized");

        // Define URLs for the two windows
        let window1_urls = vec![
            "https://www.google.com",
            "https://www.github.com",
            "https://www.rust-lang.org",
        ];
        let window2_urls = vec![
            "https://www.microsoft.com",
            "https://www.apple.com",
            "https://www.openai.com",
        ];

        // Create the first window with three tabs
        self.create_window_with_tabs(window1_urls);

        // Create the second window with three tabs
        self.create_window_with_tabs(window2_urls);

        // In your on_context_initialized, after creating windows/tabs:
        let debug_port = 9222;
        let all_urls = vec![
            vec!["https://google.com", "https://github.com"]
                .into_iter().map(String::from).collect(),
            vec!["https://rust-lang.org", "https://crates.io"]
                .into_iter().map(String::from).collect(),
        ];

        // spawn one driver task per BrowserView
        spawn_cdp_drivers(debug_port, all_urls, Duration::from_secs(10));
    }
}

impl DemoBrowserProcessHandler {
    fn create_window_with_tabs(&self, urls: Vec<&str>) {
        // Create a single browser view with the first URL
        let mut client = DemoClient::new();
        let first_url = CefString::from(urls[0]);

        // prepare window info and BrowserSettings for the call
        let mut window_info = WindowInfo::default();
        let browser_settings = BrowserSettings::default();
        let _ = browser_host_create_browser_sync(
            Some(&mut window_info),
            Some(&mut client),
            Some(&first_url),
            Some(&browser_settings),
            None::<&mut DictionaryValue>,
            None::<&mut RequestContext>,
        )
        .expect("Failed to create browser view");

        // Convert string references to owned Strings
        // let owned_urls: Vec<String> = urls.iter().map(|&s| s.to_owned()).collect();

        // // Create communication channels
        // let (command_tx, command_rx) = mpsc::channel::<WindowCommand>(32);
        // let window_proxy = WindowProxy::new(command_tx);

        // // Create the window with the single browser view
        // let mut delegate = DemoWindowDelegate::new(Arc::new(Mutex::new(mpsc::channel(10).1)));
        
        // if let Ok(mut window_opt) = self.window.lock() {
        //     // *window_opt = Some(
        //     //     window_create_top_level(Some(&mut delegate)).expect("Failed to create window")
        //     // );
            
        //     if let Some(window) = window_opt.as_mut() {
        //         // window.show();
                
        //         // Clone the browser view and window for the manager
        //         // let browser_view_clone = browser_view.clone();
        //         // let window_clone = window.clone();
                
        //         // // Start the view manager in a separate task
        //         // tokio::spawn(async move {
        //         //     let mut manager = SingleViewManager::new(
        //         //         window_clone,
        //         //         browser_view_clone, 
        //         //         owned_urls,
        //         //         command_rx
        //         //     );
        //         //     manager.run().await;
        //         // });
                
        //         // // Start a timer for automatic tab switching
        //         // let window_proxy_clone = window_proxy.clone();
        //         // let tab_count = urls.len();
        //         // tokio::spawn(async move {
        //         //     Self::start_tab_switch_timer(window_proxy_clone, tab_count).await;
        //         // });
        //     }
        // }
    }

    // Timer that uses the proxy for tab switching
    async fn start_tab_switch_timer(window_proxy: WindowProxy, tab_count: usize) {
        let mut current_tab = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            current_tab = (current_tab + 1) % tab_count;
            println!("Requesting navigation to tab {}", current_tab);
            if let Err(e) = window_proxy.switch_tab(current_tab).await {
                println!("Failed to send tab switch command: {}", e);
                break;
            }
        }
    }
}

struct DemoClient(*mut RcImpl<cef_dll_sys::_cef_client_t, Self>);

impl DemoClient {
    fn new() -> Client {
        Client::new(Self(std::ptr::null_mut()))
    }
}

impl WrapClient for DemoClient {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::_cef_client_t, Self>) {
        self.0 = object;
    }
}

impl Clone for DemoClient {
    fn clone(&self) -> Self {
        unsafe {
            let rc_impl = &mut *self.0;
            rc_impl.interface.add_ref();
        }

        Self(self.0)
    }
}

impl Rc for DemoClient {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.0;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl ImplClient for DemoClient {
    fn get_raw(&self) -> *mut cef_dll_sys::_cef_client_t {
        println!("get_raw called");
        self.0.cast()
    }
    
            fn audio_handler(&self) -> Option<AudioHandler> {
        Default::default()
            }
        
            fn command_handler(&self) -> Option<CommandHandler> {
        Default::default()
            }
        
            fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
        Default::default()
            }
        
            fn dialog_handler(&self) -> Option<DialogHandler> {
        Default::default()
            }
        
            fn display_handler(&self) -> Option<DisplayHandler> {
                println!("Display handler called {:?}", self.0);
        Default::default()
            }
        
            fn download_handler(&self) -> Option<DownloadHandler> {
        Default::default()
            }
        
            fn drag_handler(&self) -> Option<DragHandler> {
        Default::default()
            }
        
            fn find_handler(&self) -> Option<FindHandler> {
        Default::default()
            }
        
            fn focus_handler(&self) -> Option<FocusHandler> {
                println!("Focus handler called");
        Default::default()
            }
        
            fn frame_handler(&self) -> Option<FrameHandler> {
        Default::default()
            }
        
            fn permission_handler(&self) -> Option<PermissionHandler> {
        Default::default()
            }
        
            fn jsdialog_handler(&self) -> Option<JsdialogHandler> {
        Default::default()
            }
        
            fn keyboard_handler(&self) -> Option<KeyboardHandler> {
                println!("Keyboard handler called {:?}", self.0);
                Some(KeyboardHandler::new(CustomKeyboardHandler::new()))
        // Default::default()
            }
        
            fn life_span_handler(&self) -> Option<LifeSpanHandler> {
        Default::default()
            }
        
            fn print_handler(&self) -> Option<PrintHandler> {
        Default::default()
            }
        
            fn render_handler(&self) -> Option<RenderHandler> {
        Default::default()
            }
        
            fn request_handler(&self) -> Option<RequestHandler> {
        Default::default()
            }
        
            fn on_process_message_received(
        &self,
        browser: Option<&mut impl ImplBrowser>,
        frame: Option<&mut impl ImplFrame>,
        source_process: ProcessId,
        message: Option<&mut impl ImplProcessMessage>,
            ) -> ::std::os::raw::c_int {
                println!("Process message received from process: {:?}", source_process);
                if let Some(browser) = browser {
                    println!("Browser ID: {}", browser.identifier());
                } else {
                    println!("Browser: None");
                }
        Default::default()
            }
        
    fn load_handler(&self) -> Option<LoadHandler> {
        println!("Load handler requested for client {:?}", self.0);
        Some(DemoLoadHandler::new())
    }
}

pub struct DemoWindowDelegate {
    base: *mut RcImpl<cef_dll_sys::_cef_window_delegate_t, Self>,
    active_tab_index: usize,         // Track the currently active tab
    tab_switch_rx: Arc<Mutex<mpsc::Receiver<usize>>>, // Wrap receiver in Arc<Mutex<>>
    window: Option<Window>,          // Add a field to store the window instance
}

impl DemoWindowDelegate {
    fn new( tab_switch_rx: Arc<Mutex<mpsc::Receiver<usize>>>) -> WindowDelegate {
            WindowDelegate::new(Self {
                base: std::ptr::null_mut(),
                active_tab_index: 0, // Start with the first tab as active
                tab_switch_rx,
                window: None, // Initialize the window field as None
            })
        }

    fn switch_tab(&mut self, tab_index: usize) {
        // if tab_index < self.browser_views.len() {
        //     // Hide all tabs
        //     for (i, browser_view) in self.browser_views.iter_mut().enumerate() {
        //         browser_view.set_visible(if i == tab_index { 1 } else { 0 });
        //         println!("Tab {} visibility set to {}", i, i == tab_index);
        //     }
            
        //     self.active_tab_index = tab_index;
        //     println!("Switched to tab {}", tab_index);
            
        //     // Force layout update
        //     if let Some(window) = &self.window {
        //         window.layout();
        //     }
        // }
    }

    /// Start listening for tab-switching events
    async fn listen_for_tab_switches(&mut self) {
        let tab_switch_rx = self.tab_switch_rx.clone(); // Clone the Arc<Mutex<Receiver>>
        while let Some(tab_index) = tab_switch_rx.lock().unwrap().recv().await {
            println!("Switching to tab {}", tab_index);
            // if let Some(mut window) = self.window.lock().unwrap().as_mut() {
            //     self.switch_tab(window, tab_index);
            // }
        }
    }
}

impl WrapWindowDelegate for DemoWindowDelegate {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::_cef_window_delegate_t, Self>) {
        self.base = object;
    }
}

impl Clone for DemoWindowDelegate {
    fn clone(&self) -> Self {
        let base = self.base.clone();
        unsafe {
            let rc_impl = &mut *base;
            rc_impl.interface.add_ref();
        }

        Self {
            base: self.base,
            // browser_views: self.browser_views.clone(),
            tab_switch_rx: self.tab_switch_rx.clone(), // Wrap in Arc<Mutex<>> for shared ownership
            active_tab_index: self.active_tab_index,
            window: self.window.clone(), // Clone the window field
        }
    }
}

impl Rc for DemoWindowDelegate {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        unsafe {
            let base = unsafe { &*self.base };
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl ImplViewDelegate for DemoWindowDelegate {
    fn on_child_view_changed(
        &self,
        _view: Option<&mut impl ImplView>,
        _added: ::std::os::raw::c_int,
        _child: Option<&mut impl ImplView>,
    ) {
        // view.as_panel().map(|x| x.as_window().map(|w| w.close()));
    }

    fn get_raw(&self) -> *mut cef_dll_sys::_cef_view_delegate_t {
        self.base.cast()
    }
}

impl ImplPanelDelegate for DemoWindowDelegate {}

impl ImplWindowDelegate for DemoWindowDelegate {
fn on_window_created(&self, window: Option<&mut impl ImplWindow>) {
    if let Some(window) = window {
        if let Some(mut view) = window.as_browser_view() {
            window.add_child_view(Some(&mut view));
            window.show();
        } else {
            println!("Window is None inside in on_window_created");
        }
    } else {
        println!("Window is None in on_window_created");
    }
}

    fn on_window_destroyed(&self, _window: Option<&mut impl ImplWindow>) {
        quit_message_loop();
    }

    fn with_standard_window_buttons(
        &self,
        _window: Option<&mut impl ImplWindow>,
    ) -> ::std::os::raw::c_int {
        1
    }

    fn can_resize(&self, _window: Option<&mut impl ImplWindow>) -> ::std::os::raw::c_int {
        1
    }

    fn can_maximize(&self, _window: Option<&mut impl ImplWindow>) -> ::std::os::raw::c_int {
        1
    }

    fn can_minimize(&self, _window: Option<&mut impl ImplWindow>) -> ::std::os::raw::c_int {
        1
    }

    fn can_close(&self, _window: Option<&mut impl ImplWindow>) -> ::std::os::raw::c_int {
        1
    }

    fn on_key_event(
        &self,
        window: Option<&mut impl ImplWindow>,
        event: Option<&KeyEvent>,
    ) -> ::std::os::raw::c_int {
                    if let Some(event) = event {
                        println!(
                            "Key Event: windows_key_code={}, modifiers={}",
                            event.windows_key_code, event.modifiers
                        );
                    } else {
                        println!("Key Event: None");
                    }
        if let Some(window) = window {
            if let Some(event) = event {
                if event.windows_key_code == 9 && (event.modifiers & cef_dll_sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN as u32) != 0 {
                    // Switch to the next tab
                    let next_tab = 0;
                    println!("Switching to tab {}", next_tab);
                    // if let Some(delegate) = window.delegate::<DemoWindowDelegate>() {
                    //     let mut delegate = delegate.clone();
                    //     delegate.switch_tab(next_tab);
                    // }
                }
            }
        }
        0
    }
}

pub struct CefModule {
    sandbox: SandboxInfo,
    args: Args,
}

impl CefModule {
    /// Creates a new `CefModule` instance.
    pub fn new() -> Self {
        println!("CefModule::new()");
        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
        let sandbox = SandboxInfo::new();
        let args = Args::new();

        Self { sandbox, args }
    }

    /// Handles subprocess execution.
    pub fn handle_subprocess(&mut self) -> bool {
        // let cmd = self.args.as_cmd_line().unwrap();
        // if cmd.has_switch(Some(&CefString::from("type"))) != 0 {
        //     println!("CEF subprocess detected");
        //     let ret = execute_process(
        //         Some(self.args.as_main_args()),
        //         Some(&mut self.browser), // Pass the browser as a mutable reference wrapped in Some
        //         self.sandbox.as_mut_ptr(),
        //     );
        //     std::process::exit(ret);
        // }
         false
    }

    /// Initializes the CEF library for the main process.
    pub fn initialize(&mut self) {
        // let settings = Settings::default();
        // assert_eq!(
        //     initialize(
        //         Some(self.args.as_main_args()),
        //         Some(&settings),
        //         Some(&mut self.browser), // Pass the CefBrowser instance for initialization
        //         self.sandbox.as_mut_ptr()  None, // Add the missing fourth argument
            
        //     1,
        //     "Failed to initialize CEF"
        // );
        println!("CEF initialized successfully");
    }

    /// Shuts down the CEF library.
    pub fn shutdown(&self) {
        shutdown();
        println!("CEF shutdown completed");
    }

    /// Launches a `CefBrowser` instance.
    pub fn launch_browser(&mut self, url: &str) {
        // self.browser.launch(url);
    }
}

// Define the commands that can be sent to the window
#[derive(Debug)]
enum WindowCommand {
    SwitchTab(usize),
    CloseWindow,
    // Add more commands as needed
}

// WindowProxy manages communication with the actual Window
#[derive(Clone)]
struct WindowProxy {
    command_tx: Sender<WindowCommand>,
}

impl WindowProxy {
    fn new(command_tx: Sender<WindowCommand>) -> Self {
        Self { command_tx }
    }

    // Send a command to switch to a specific tab
    async fn switch_tab(&self, tab_index: usize) -> Result<(), tokio::sync::mpsc::error::SendError<WindowCommand>> {
        self.command_tx.send(WindowCommand::SwitchTab(tab_index)).await
    }

    // Send a command to close the window
    async fn close_window(&self) -> Result<(), tokio::sync::mpsc::error::SendError<WindowCommand>> {
        self.command_tx.send(WindowCommand::CloseWindow).await
    }
}

struct WindowManager {
    window: Window,
    browser_views: Vec<BrowserView>,
    active_tab_index: usize,
    command_rx: Receiver<WindowCommand>,
}

impl WindowManager {
    fn new(window: Window, mut browser_views: Vec<BrowserView>, command_rx: Receiver<WindowCommand>) -> Self {
        // Add ONLY the first view initially
        if let Some(first_view) = browser_views.first_mut() {
            window.add_child_view(Some(first_view));
        }
        window.layout();
        
        Self {
            window,
            browser_views,
            active_tab_index: 0,
            command_rx,
        }
    }

    async fn run(&mut self) {
        // Don't add all views here - we already added the first one in new()
        
        while let Some(command) = self.command_rx.recv().await {
            match command {
                WindowCommand::SwitchTab(tab_index) => {
                    self.switch_tab(tab_index);
                },
                WindowCommand::CloseWindow => {
                    break;
                }
            }
        }
    }

    fn switch_tab(&mut self, tab_index: usize) {
        if tab_index < self.browser_views.len() && tab_index != self.active_tab_index {
            // Remove the current view
            if let Some(current_view) = self.browser_views.get_mut(self.active_tab_index) {
                self.window.remove_child_view(Some(current_view));
            }
            
            // Add the new view
            if let Some(new_view) = self.browser_views.get(tab_index) {
                if let Some(new_view) = self.browser_views.get_mut(tab_index) {
                    self.window.add_child_view(Some(new_view));
                    self.active_tab_index = tab_index;
                    println!("Switched to tab {}", tab_index);
                    
                    // Force layout update
                    self.window.layout();
                }
                self.active_tab_index = tab_index;
                println!("Switched to tab {}", tab_index);
                
                // Force layout update
                self.window.layout();
            }
        }
    }
}

// Define a simplified structure that manages a single browser view
struct SingleViewManager {
    browser_view: BrowserView,
    urls: Vec<String>,
    current_index: usize,
    command_rx: Receiver<WindowCommand>,
}

impl SingleViewManager {
    fn new(window: Window, mut browser_view: BrowserView, urls: Vec<String>, command_rx: Receiver<WindowCommand>) -> Self {
        // Add the single browser view to the window
        window.add_child_view(Some(&mut browser_view));
        window.layout();
        
        Self {
            browser_view,
            urls,
            current_index: 0,
            command_rx,
        }
    }

    async fn run(&mut self) {
        while let Some(command) = self.command_rx.recv().await {
            match command {
                WindowCommand::SwitchTab(tab_index) => {
                    self.navigate_to_tab(tab_index);
                },
                WindowCommand::CloseWindow => {
                    break;
                }
            }
        }
    }

    fn navigate_to_tab(&mut self, tab_index: usize) {
        if tab_index < self.urls.len() && tab_index != self.current_index {
            // Get the URL for the requested tab
            let url = &self.urls[tab_index];
            println!("Attempting to navigate to: {}", url);
            
            // Create a CefString from the target URL
            let cef_url = CefString::from(url.as_str());
            
            // Navigate the browser to the new URL
            if let Some(browser) = self.browser_view.browser() {
                
                if let Some(main_frame) = browser.main_frame() {
                    
                    // Load the new URL (use the CefString we created)
                    main_frame.load_url(Some(&cef_url));
                    self.current_index = tab_index;
                    println!("Navigation command sent for tab {}: {}", tab_index, url);
                } else {
                    println!("ERROR: Could not get main frame");
                }
            } else {
                println!("ERROR: Could not get browser from view");
            }
        } else if tab_index == self.current_index {
            println!("Already on tab {}", tab_index);
        } else {
            println!("Invalid tab index: {}", tab_index);
        }
    }
}

// Add this struct to your code
struct DemoLoadHandler(*mut RcImpl<cef_dll_sys::_cef_load_handler_t, Self>);

impl DemoLoadHandler {
    fn new() -> LoadHandler {
        LoadHandler::new(Self(std::ptr::null_mut()))
    }
}
// Define a struct to handle keyboard events
struct CustomKeyboardHandler(*mut RcImpl<cef_dll_sys::_cef_keyboard_handler_t, Self>);

impl CustomKeyboardHandler {
    fn new() -> Self {
        Self(std::ptr::null_mut())
    }
}

impl Rc for CustomKeyboardHandler {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.0;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for CustomKeyboardHandler {
    fn clone(&self) -> Self {
        unsafe {
            let rc_impl = &mut *self.0;
            rc_impl.interface.add_ref();
        }
        Self(self.0)
    }
}

impl WrapKeyboardHandler for CustomKeyboardHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::_cef_keyboard_handler_t, Self>) {
        self.0 = object;
    }
}

impl ImplKeyboardHandler for CustomKeyboardHandler {
    fn on_key_event(
        &self,
        browser: Option<&mut impl ImplBrowser>,
        event: Option<&KeyEvent>,
        tag_msg: Option<&mut cef_dll_sys::tagMSG>,
    ) -> ::std::os::raw::c_int {
        let _ = tag_msg;
        if let Some(event) = event {
            println!(
                "Key Event: windows_key_code={}, modifiers={}",
                event.windows_key_code, event.modifiers
            );
            if event.windows_key_code == 'Q' as i32 && (event.modifiers & KeyModifiers::EVENTFLAG_CONTROL_DOWN as u32) != 0 {
                println!("Exiting application...");
                // instead of exit(0), close the browser and quit CEF cleanly:
                if let Some(browser) = browser {
                    if let Some(host) = browser.host() {
                        // this will close the window (and when the last browser goes away, CEF will shut down)
                        host.close_browser(1);
                    }
                }
                println!("Browser closed, exiting application...");
                // now tell CEF to break out of run_message_loop()
                quit_message_loop();
                println!("CEF message loop quit, exiting application...");
                // std::process::exit(0);
            }
        }
        0
    }
    fn get_raw(&self) -> *mut cef_dll_sys::_cef_keyboard_handler_t {
        self.0.cast()
    }
}

impl WrapLoadHandler for DemoLoadHandler {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::_cef_load_handler_t, Self>) {
        self.0 = object;
    }
}

impl Clone for DemoLoadHandler {
    fn clone(&self) -> Self {
        unsafe {
            let rc_impl = &mut *self.0;
            rc_impl.interface.add_ref();
        }

        Self(self.0)
    }
}

impl Rc for DemoLoadHandler {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.0;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl ImplLoadHandler for DemoLoadHandler {
    fn get_raw(&self) -> *mut cef_dll_sys::_cef_load_handler_t {
        self.0.cast()
    }

    fn on_loading_state_change(
        &self,
        browser: Option<&mut impl ImplBrowser>,
        is_loading: ::std::os::raw::c_int,
        can_go_back: ::std::os::raw::c_int,
        can_go_forward: ::std::os::raw::c_int,
    ) {
        println!("🔄 Loading state changed:");
        if let Some(browser) = browser {
            println!("   Browser ID: {}", browser.identifier());
        }
        println!("   Is loading: {}", is_loading != 0);
        println!("   Can go back: {}", can_go_back != 0);
        println!("   Can go forward: {}", can_go_forward != 0);
    }

    fn on_load_start(
        &self,
        browser: Option<&mut impl ImplBrowser>,
        frame: Option<&mut impl ImplFrame>,
        transition_type: TransitionType,
    ) {
        println!("▶️ Load started:");
        if let Some(browser) = browser {
            println!("   Browser ID: {}", browser.identifier());
        }
        if let Some(frame) = frame {
            // println!("   Frame URL: {}", frame.url());
            println!("   Is main frame: {}", frame.is_main() != 0);
        }
        // println!("   Transition type: {}", transition_type);
    }

    fn on_load_end(
        &self,
        browser: Option<&mut impl ImplBrowser>,
        frame: Option<&mut impl ImplFrame>,
        http_status_code: ::std::os::raw::c_int,
    ) {
        println!("✅ Load completed:");
        if let Some(browser) = browser {
            println!("   Browser ID: {}", browser.identifier());
        }
        if let Some(frame) = frame {
            // println!("   Frame URL: {}", frame.get_url().to_string());
            println!("   Is main frame: {}", frame.is_main() != 0);
        }
        println!("   HTTP status code: {}", http_status_code);
    }

    fn on_load_error(
        &self,
        browser: Option<&mut impl ImplBrowser>,
        frame: Option<&mut impl ImplFrame>,
        // error_code: Errorcode,
        error_code: cef::Errorcode,
        error_text: Option<&CefString>,
        failed_url: Option<&CefString>,
    ) {
        println!("❌ Load error:");
        if let Some(browser) = browser {
            println!("   Browser ID: {}", browser.identifier());
        }
        if let Some(frame) = frame {
            // if let Some(url) = frame.url() {
            //     println!("   Frame URL: {}", url.to_string());
            // } else {
            //     println!("   Frame URL: None");
            // }
            println!("   Is main frame: {}", frame.is_main() != 0);
        }
        println!("   Error code: {:?}", error_code);
        if let Some(text) = error_text {
            println!("   Error text: {}", text.to_string());
        }
        if let Some(url) = failed_url {
            println!("   Failed URL: {}", url.to_string());
        }
    }
}

// helper to discover targets
async fn fetch_targets(port: u16) -> serde_json::Value {
    let url = format!("http://127.0.0.1:{}/json", port);
    let resp = reqwest::get(&url).await.unwrap();
    resp.json().await.unwrap()
}

// drive one view by its DevTools WS URL
async fn drive_cdp(ws_url: String, urls: Vec<String>, delay: Duration) {
    let (mut ws, _) = connect_async(ws_url).await.unwrap();
    // enable Page domain
    ws.send(tungstenite::Message::Text(
        json!({"id":1,"method":"Page.enable"}).to_string().into()
    )).await.unwrap();

    let mut i = 0;
    loop {
      let url = &urls[i];
      sleep(delay).await;
      let cmd = json!({
        "id": 1000 + i,
        "method": "Page.navigate",
        "params": { "url": url }
      });
      println!("→ navigating to {}", url);
      ws.send(tungstenite::Message::Text(cmd.to_string().into())).await.unwrap();
      i = (i + 1) % urls.len();
    }
}

fn spawn_cdp_drivers(
    debug_port: u16,
    view_urls: Vec<Vec<String>>,
    per_tab_delay: Duration
) {
    for (window_index, urls) in view_urls.into_iter().enumerate() {
        let port = debug_port;
        task::spawn(async move {
            loop {
                let targets = fetch_targets(port).await;
                if let Some(arr) = targets.as_array() {
                    // once CEF has registered *at least* window_index+1 targets…
                    if arr.len() > window_index {
                        let entry = &arr[window_index];
                        let ws_url = entry["webSocketDebuggerUrl"]
                            .as_str()
                            .expect("no ws url")
                            .to_string();
                        println!("→ window {} uses {}", window_index, ws_url);
                        drive_cdp(ws_url, urls.clone(), per_tab_delay).await;
                        break;
                    }
                }
                sleep(Duration::from_secs(1)).await;
            }
        });
    }
}

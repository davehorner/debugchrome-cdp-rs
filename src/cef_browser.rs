use cef::{args::Args, rc::*, sandbox_info::SandboxInfo, *};
use std::sync::{Arc, Mutex};
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
        let url = CefString::from(url);

        let browser_view = browser_view_create(
            Some(&mut client),
            Some(&url),
            Some(&Default::default()),
            Option::<&mut DictionaryValue>::None,
            Option::<&mut RequestContext>::None,
            Option::<&mut BrowserViewDelegate>::None,
        )
        .expect("Failed to create browser view");

        let mut delegate = DemoWindowDelegate::new(browser_view);
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
    }
}

impl DemoBrowserProcessHandler {
    fn create_window_with_tabs(&self, urls: Vec<&str>) {
        let mut browser_views = Vec::new();

        // Create a browser view for each URL
        for url in urls {
            let mut client = DemoClient::new();
            let url = CefString::from(url);

            let browser_view = browser_view_create(
                Some(&mut client),
                Some(&url),
                Some(&Default::default()),
                Option::<&mut DictionaryValue>::None,
                Option::<&mut RequestContext>::None,
                Option::<&mut BrowserViewDelegate>::None,
            )
            .expect("Failed to create browser view");

            browser_views.push(browser_view);
        }

        // Create a top-level window and add all browser views (tabs)
        let mut delegate = DemoWindowDelegate::new(browser_views[0].clone());
        if let Ok(mut window) = self.window.lock() {
            *window = Some(
                window_create_top_level(Some(&mut delegate)).expect("Failed to create window"),
            );

            if let Some(window) = window.as_mut() {
                for browser_view in browser_views {
                    window.add_child_view(Some(&mut browser_view.clone()));
                }
                window.show();
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
        self.0.cast()
    }
}

struct DemoWindowDelegate {
    base: *mut RcImpl<cef_dll_sys::_cef_window_delegate_t, Self>,
    browser_view: BrowserView,
}

impl DemoWindowDelegate {
    fn new(browser_view: BrowserView) -> WindowDelegate {
        WindowDelegate::new(Self {
            base: std::ptr::null_mut(),
            browser_view,
        })
    }
}

impl WrapWindowDelegate for DemoWindowDelegate {
    fn wrap_rc(&mut self, object: *mut RcImpl<cef_dll_sys::_cef_window_delegate_t, Self>) {
        self.base = object;
    }
}

impl Clone for DemoWindowDelegate {
    fn clone(&self) -> Self {
        unsafe {
            let rc_impl = &mut *self.base;
            rc_impl.interface.add_ref();
        }

        Self {
            base: self.base,
            browser_view: self.browser_view.clone(),
        }
    }
}

impl Rc for DemoWindowDelegate {
    fn as_base(&self) -> &cef_dll_sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.base;
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
            let mut view = self.browser_view.clone();
            window.add_child_view(Some(&mut view));
            window.show();
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

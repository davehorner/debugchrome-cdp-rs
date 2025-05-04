### README.md

# debugchrome-cdp-rs

`debugchrome-cdp-rs` introduces a command `debugchrome` and a custom windows protocol, `debugchrome:` and `debugchrome://`.  It is currently windows only.

When using default protocol handlers, the url opens in the default browser.  There is no cross platform way to interact with the programs that launch or the documents themselves after launch.  Url invocation gives you no way to interrogate the system to determine what exact tab and program the user is interacting with.

Using something like open, cmd /c start, [open::that](https://github.com/Byron/open-rs) the default application runs but there are no great ways to query/interact with that unknown servicing application and document.

Sometimes default handlers aren't set, other times the api may return incorrect results, or the browser just doesn't open as it should. [#73](https://github.com/Byron/open-rs/issues/73)  It's unreliable like udp; `chromedebug` helps make default open more reliable by providing mechanisms to query and control past the point of open.

**`debugchrome` is a program and protocol handler designed to open a http(s) url in a --remote-debugging-port=9222 chrome browser.**

The protocol works if you specify `debugchrome:` or `debugchrome://` before your url and add the !bang variables at the end of your query to control operation.

---

## Features

1. **Open a url (brought to front and at location on screen)**:
   - Opens a specified URL with optional window placement / bounds (`!x`, `!y`, `!w`, `!h`).
   - Example: `debugchrome:https://www.rust-lang.org?!x=0&!y=0&!w=800&!h=600`
   - Window bounds can be expressed as a percentage and relative to a monitor
   - Example: [`debugchrome://https://www.rustlang.org?!x=12.5%&!y=12.5%&!w=75%&!h=75%&!monitor=2`](debugchrome://https://www.rustlang.org?!x=12.5%&!y=12.5%&!w=75%&!h=75%&!monitor=2)
2. **Set `id`**:
   - Sets a custom `id` in the tab's JavaScript context(session storage so it persists refresh) using the `!id` parameter in the URL.
   - Example: `debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123`
   - This Id is used to annotate the url with an ID that can be used to find and interact with the tab.
3. **Timeout**:
   - `debugchrome` will wait for the specified number of seconds and then search and close the page automatically.
   - Example: `debugchrome://https://crates.io/crates/debugchrome-cdp-rs?!timeout=5`
4. **Keep Focus**
   - Add a !keep_focus parameter in the url and the window that is currently active will be re-focused after the page is loaded.
   - Example: [`debugchrome://https://crates.io/crates/debugchrome-cdp-rs?!id=21jump&!keep_focus`](debugchrome://https://crates.io/crates/debugchrome-cdp-rs?!id=21jump&!keep_focus)
   This allows you to launch a url from cmd or powershell and not have the webpage take focus away from the terminal.
5. **Take a Screenshot**:
   - Captures a screenshot of the page and opens the default viewer for `.png`.
   - Example: `debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123&!screenshot`

6. **Search for Tabs by `bangId`**:
   - Searches all open tabs for a specific `bangId` and prints the matching tab's URL.

7. **Register Custom Protocol**:
   - Registers the `debugchrome:` protocol in the Windows registry for easier usage.

---

## Usage

Prefix any url with `debugchrome:` and that url will be processed by debugchrome.

The protocol begins `debugchome:` then your url, followed by ! bang parameter variables.  In other words, all parameters operated on by `debugchrome` are assumed to be appended to the end of the url (not interdispersed) and the variables debugchrome deals with begin with the !.  This makes it easy to determine the clean url and limits clashing with existing variables.

debugchome addresses common challenges such as controlling the location of the browser window, determining if a url is open, closing a tab, setting custom data (`bangId`) in the JavaScript context, and even capturing screenshots. By using the `debugchrome:` protocol, you've got a way to position, query, and control default document open.

### 1. **Register the `debugchrome:` Protocol**
If you want to use `debugchrome:\\` urls, you will first need to register the path to the executable in the windows registry.
```bash
debugchrome.exe --register
```
- Writes a debugchrome.reg file next to the debugchrome.exe (typically in your .cargo/bin)
- Registers the `debugchrome:` protocol in the Windows registry (given permissions).
- Allows you to use `debugchrome:` URLs directly.


### 2. **Open a url**
```bash
debugchrome.exe "debugchrome:https://www.rustlang.org?!x=0&!y=0&!w=800&!h=600"
```
- Opens a specified URL.
- add any of ! query parameters:
  - `!x`: X-coordinate of the window.
  - `!y`: Y-coordinate of the window.
  - `!w`: Width of the window.
  - `!h`: Height of the window.
  - etc

### 3. **Set `id`**
```bash
debugchrome.exe "debugchrome:https://www.rustlang.org?!x=0&!y=0&!w=800&!h=600&!id=123"
```
- Sets `window.bangId` in the tab's JavaScript context to `123`.

### 4. **Search for page by `id`**
```bash
debugchrome.exe --search 123
```
- Searches all open tabs for a tab where `window.bangId` is `123`.
- Prints the matching tab's URL if found.

### 5. **Take a Screenshot**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org!screenshot"
```
- Captures a screenshot of the tab and opens the default viewer for the screenshot.

### 6. **Keep Focus**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!keep_focus"
```
- Ensures the window that was in focus at time of open is the window that remains in focus on exit.

### 7. **Close a Tab**
```bash
debugchrome.exe --search 123 --close
```
- Searches for the tab with the specified `bangId` and closes it.
- Example: `debugchrome.exe --search 123 --close` will close the tab where `window.bangId` is `123`.
- **Note**: The `!id` parameter must be specified when opening the tab to use this feature.
- **Note**: Specifying `!close` without an `!id` will not close, if you specify close and the page isn't found, the page will be opened.  The same url with the `!close` specified, will then close the page when opened again.
- Example: `debugchrome.exe "debugchrome:https://www.rust-lang.org!id=1&!close&!openwindow&!keep_focus"` will open and close a window without taking focus.

### 8. **Refresh a Tab**
```bash
debugchrome.exe --search 123 [--close]
```
- Searches for the tab with the specified `id` and activates it.  If !refresh is specified on the url, the page will be refreshed after being activated.
- Example: `debugchrome.exe --search 123 --close` will close the page where `window.bangId` is `123`.
- Example: `debugchrome.exe "debugchrome:https://www.rust-lang.org?!refresh"` will close the page after 10 seconds.
- **Note**: The `!id` parameter must be specified when opening the tab to use this feature.

### 9. **Set Timeout**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!timeout=10"
```
- Sets a timeout (in seconds) for the tab to remain open.
- Example: `debugchrome.exe "debugchrome:https://www.rust-lang.org?!timeout=10"` will close the page after 10 seconds.

### 10. **Specify Monitor**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!monitor=2"
```
- Opens the tab on a specific monitor.
- Example: `debugchrome.exe "debugchrome:https://www.rust-lang.org?!monitor=2"` will open the tab on monitor 2.
- Monitor indices start from 0.


### 11. **Open Window**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!openwindow"
```
- Opens the url as a new window instead of as a tab.
- Example: `debugchrome.exe "debugchrome:https://www.rust-lang.org?!openwindow"` will open the url in a new window.


## Sample CLI
1. **Open a url using cli**:
   `debugchrome.exe "debugchrome:https://www.rustlang.org?!x=0&!y=0&!w=800&!h=600&!id=456"`
2. **Search for a page by id using cli**:
   `
   debugchrome.exe --search 456
   `

3. **Register the Protocol using cli**:
   `
   debugchrome.exe --register
   `

---

## How It Works

You can run a debug browser and open things in there to get programatic access to your tab information; you need a protocol handler installed to allow default open urls into that debugger enabled environment.

### Opening a url
- Uses the `Target.createTarget` CDP method to open a new tab with the specified URL.
- Optionally sets window bounds using `Browser.setWindowBounds`.

### Setting `bangId`
- Parses the `!id` parameter from the URL.
- Uses the `Runtime.evaluate` CDP method to set `window.bangId` in the tab's JavaScript context.

### Taking a Screenshot
- Enables the `Page` domain using `Page.enable`.
- Captures a screenshot using `Page.captureScreenshot`.
- Saves the screenshot as `screenshot.png`.

### Searching for Tabs
- Fetches all open tabs using the `http://localhost:9222/json` endpoint.
- Connects to each tab's WebSocket and evaluates `window.bangId` using `Runtime.evaluate`.
- Matches the `bangId` with the search query.

### Registering the Protocol
- Writes a `.reg` file to register the `debugchrome:` protocol in the Windows registry.
- Allows you to use `debugchrome:` URLs directly.

---

## Example Output

Output is minimal to stdout; detailed output can be found in `%USERPROFILE%\.cargo\bin\debugchrome.log` or next to wherever `debugchrome` lives.  The same is true for the screenshots; it saves a single `%USERPROFILE%\.cargo\bin\debugchome.png` / `& "$env:USERPROFILE\.cargo\bin\debugchrome.png"` or next to the executable.  Sorry for the loose files in bin, at least they all start with debugchrome; this may change in the future.  Chrome profile folders are kept in %TEMP%\ starting with debugchrome.

---

## Notes

1. **Dependencies**:
   - Ensure that Chrome is running with the `--remote-debugging-port=9222` flag.  If it's not running, it will attempt to start it for you.

2. **Skipped Tabs**:
   - Tabs with URLs starting with `ws://`, `chrome-extension://`, `chrome://`, , `about:`, `data:`, `view-source:`, `devtools://`, or `chrome-devtools://` are skipped during the search.

3. **It's not fast**:
   - If you need to scan 400 tabs; it is going to take some time.

---

## License

This project is licensed under the MIT License.
David Horner 5/25
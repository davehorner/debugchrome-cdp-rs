### README.md

# debugchrome-cdp-rs

`debugchrome-cdp-rs` introduces a command `debugchrome` and a custom windows protocol, `debugchrome:`.

When using default protocol handlers, the url opens in the default browser.  There is no cross platform way to interact with the programs that launch or the documents themselves after launch.  Url invocation gives you no way to interrogate the system to determine what exact tab and program the user is interacting with.

Using something like open, cmd /c start, [open::that](https://github.com/Byron/open-rs) the default application runs but there are no great way to query/interact with that unknown servicing application and document.

Sometimes defaults aren't set, other times the api may return incorrect results, or the browser just doesn't open as it should. [#73](https://github.com/Byron/open-rs/issues/73)  It's unreliable like udp.

**`debugchrome` is a program and protocol handler designed to open a http(s) url in a --remote-debugging-port=9222 chrome browser.**

The protocol works if you specify `debugchrome:` or `debugchrome://`.

---

## Features

1. **Open a Chrome Tab**:
   - Opens a new Chrome tab with a specified URL and optional window bounds (`!x`, `!y`, `!w`, `!h`).
   - Example: `debugchrome:https://www.rust-lang.org?!x=0&!y=0&!w=800&!h=600`
   - Window bounds can be expressed as a percentage and relative to a monitor
   - Percentage and monitor: `debugchrome:https://www.rustlang.org?!x=12.5%&!y=12.5%&!w=75%&!h=75%&!monitor=2`
2. **Set `bangId`**:
   - Sets a custom `bangId` in the tab's JavaScript context using the `!id` parameter in the URL.
   - Example: `debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123`
   - This Id is used to annotate the url with an ID that can be used to find and interact with the tab.
3. **Take a Screenshot**:
   - Captures a screenshot of the opened tab and saves it as `screenshot.png`.

4. **Search for Tabs by `bangId`**:
   - Searches all open tabs for a specific `bangId` and prints the matching tab's URL.

5. **Register Custom Protocol**:
   - Registers the `debugchrome:` protocol in the Windows registry for easier usage.

---

## Usage

Prefix any url with `debugchrome:` and that url will be processed by debugchrome.

The protocol begins `debugchome:` then your url, followed by ! bang parameter variables.  In other words, all parameters operated on by `debugchrome` are assumed to be appended to the end of the url (not interdispersed) and the variables debugchrome deals with begin with the !.  This makes it easy to determine the clean url and limits clashing with existing variables.

debugchome addresses common challenges such as controlling the location of the browser window, determining if a url is open in the tabs, closing a tab, setting custom data (`bangId`) in the JavaScript context, and even capturing screenshots, and searching for specific tabs. By using the `debugchrome:` protocol, you've got a way to position, query, and control default document open.

### 1. **Register the `debugchrome:` Protocol**
```bash
debugchrome.exe --register
```
- Registers the `debugchrome:` protocol in the Windows registry.
- Allows you to use `debugchrome:` URLs directly.


### 2. **Open a Chrome Tab**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!x=0&!y=0&!w=800&!h=600"
```
- Opens a new Chrome tab with the specified URL.
- Optional query parameters:
  - `!x`: X-coordinate of the window.
  - `!y`: Y-coordinate of the window.
  - `!w`: Width of the window.
  - `!h`: Height of the window.

### 3. **Set `bangId`**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!x=0&!y=0&!w=800&!h=600&!id=123"
```
- Sets `window.bangId` in the tab's JavaScript context to `123`.

### 4. **Search for Tabs by `bangId`**
```bash
debugchrome.exe --search 123
```
- Searches all open tabs for a tab where `window.bangId` is `123`.
- Prints the matching tab's URL if found.

### 5. **Take a Screenshot**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org"
```
- Captures a screenshot of the opened tab and saves it as `screenshot.png`.

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

### 8. **Refresh a Tab**
```bash
debugchrome.exe --search 123 --refresh
```
- Searches for the tab with the specified `bangId` and refreshes it.
- Example: `debugchrome.exe --search 123 --refresh` will refresh the tab where `window.bangId` is `123`.
- **Note**: The `!id` parameter must be specified when opening the tab to use this feature.

### 9. **Set Timeout**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!timeout=10&!id=123"
```
- Sets a timeout (in seconds) for the tab to remain open.
- Example: `debugchrome.exe "debugchrome:https://www.rust-lang.org?!timeout=10&!id=123"` will close the tab after 10 seconds.
- **Note**: The `!id` parameter must be specified when opening the tab to use this feature.

### 10. **Specify Monitor**
```bash
debugchrome.exe "debugchrome:https://www.rust-lang.org?!monitor=2&!id=123"
```
- Opens the tab on a specific monitor.
- Example: `debugchrome.exe "debugchrome:https://www.rust-lang.org?!monitor=2&!id=123"` will open the tab on monitor 2.
- Monitor indices start from 1.
- **Note**: The `!id` parameter must be specified when opening the tab to use this feature.

## Sample URLs

1. **Open a Tab with Bounds**:
   ```bash
   debugchrome.exe "debugchrome:https://www.rust-lang.org?x=100&y=100&w=1024&h=768"
   ```

2. **Open a Tab and Set `bangId`**:
   ```bash
   debugchrome.exe "debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=456"
   ```

3. **Search for a Tab by `bangId`**:
   ```bash
   debugchrome.exe --search 456
   ```

4. **Register the Protocol**:
   ```bash
   debugchrome.exe --register
   ```

---

## How It Works

### Opening a Tab
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

### Opening a Tab and Setting `bangId`:
```plaintext
Requested debug Chrome with URL: https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123
Setting bangId in the tab... https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123
Set window.bangId to 123
Sent command to verify bangId
Received message: {"id":4,"result":{"result":{"type":"string","value":"123"}}}
Successfully verified bangId: 123
```

### Searching for a Tab by `bangId`:
```plaintext
Searching for bangId = 123
Searching tab: https://www.rust-lang.org
Connected to WebSocket URL: ws://localhost:9222/devtools/page/abc123
Sent command to get bangId with id 5
Received message: {"id":5,"result":{"result":{"type":"string","value":"123"}}}
Found tab with bangId 123: https://www.rust-lang.org
```

### Taking a Screenshot:
```plaintext
Sending captureScreenshot command: {"id":2,"method":"Page.captureScreenshot"}
Current directory: "C:\\path\\to\\debugchrome"
Screenshot saved to screenshot.png
```

---

## Notes

1. **Dependencies**:
   - Ensure that Chrome is running with the `--remote-debugging-port=9222` flag.

2. **Error Handling**:
   - If the `!id` parameter is missing, an error is logged, and the program continues execution.

3. **Skipped Tabs**:
   - Tabs with URLs starting with `ws://`, `chrome-extension://`, `chrome://`, , `about:`, `data:`, `view-source:`, `devtools://`, or `chrome-devtools://` are skipped during the search.

4. **Timeouts**:
   - A 5-second timeout is enforced for WebSocket operations to prevent hanging.

---

## License

This project is licensed under the MIT License.
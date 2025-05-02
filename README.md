### README.md

# debugchrome-cdp-rs

`debugchrome-cdp-rs` is a Rust-based tool for interacting with Google Chrome via the Chrome DevTools Protocol (CDP). It allows you to open Chrome tabs, set custom data (`bangId`) in the tab's JavaScript context, take screenshots, and search for tabs based on the `bangId`.

---

## Features

1. **Open a Chrome Tab**:
   - Opens a new Chrome tab with a specified URL and optional window bounds (`x`, `y`, `w`, `h`).
   - Example: `debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600`

2. **Set `bangId`**:
   - Sets a custom `bangId` in the tab's JavaScript context using the `!id` parameter in the URL.
   - Example: `debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123`

3. **Take a Screenshot**:
   - Captures a screenshot of the opened tab and saves it as `screenshot.png`.

4. **Search for Tabs by `bangId`**:
   - Searches all open tabs for a specific `bangId` and prints the matching tab's URL.

5. **Register Custom Protocol**:
   - Registers the `debugchrome:` protocol in the Windows registry for easier usage.

---

## Usage

### 1. **Open a Chrome Tab**
```bash
debugchrome-cdp-rs.exe "debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600"
```
- Opens a new Chrome tab with the specified URL.
- Optional query parameters:
  - `x`: X-coordinate of the window.
  - `y`: Y-coordinate of the window.
  - `w`: Width of the window.
  - `h`: Height of the window.

### 2. **Set `bangId`**
```bash
debugchrome-cdp-rs.exe "debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=123"
```
- Sets `window.bangId` in the tab's JavaScript context to `123`.

### 3. **Search for Tabs by `bangId`**
```bash
debugchrome-cdp-rs.exe --search 123
```
- Searches all open tabs for a tab where `window.bangId` is `123`.
- Prints the matching tab's URL if found.

### 4. **Take a Screenshot**
```bash
debugchrome-cdp-rs.exe "debugchrome:https://www.rust-lang.org"
```
- Captures a screenshot of the opened tab and saves it as `screenshot.png`.

### 5. **Register the `debugchrome:` Protocol**
```bash
debugchrome-cdp-rs.exe --register
```
- Registers the `debugchrome:` protocol in the Windows registry.
- Allows you to use `debugchrome:` URLs directly.

---

## Sample URLs

1. **Open a Tab with Bounds**:
   ```bash
   debugchrome-cdp-rs.exe "debugchrome:https://www.rust-lang.org?x=100&y=100&w=1024&h=768"
   ```

2. **Open a Tab and Set `bangId`**:
   ```bash
   debugchrome-cdp-rs.exe "debugchrome:https://www.rust-lang.org?x=0&y=0&w=800&h=600&!id=456"
   ```

3. **Search for a Tab by `bangId`**:
   ```bash
   debugchrome-cdp-rs.exe --search 456
   ```

4. **Register the Protocol**:
   ```bash
   debugchrome-cdp-rs.exe --register
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
Current directory: "C:\\path\\to\\debugchrome-cdp-rs"
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
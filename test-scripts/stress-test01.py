"""
stress-test01.py

Description:
    A stress‐test script for the debugchrome protocol. It divides each
    connected monitor into a grid of windows and opens a placeholder
    image in each cell via debugchrome.exe.

Dependencies:
    - Python 3.8+
    - screeninfo          (pip install screeninfo)
    - ctypes              (part of the standard library)
    - concurrent.futures  (part of the standard library)
    - debugchrome.exe     (must be in PATH or specify full path)

Usage:
    python stress-test01.py

Configuration:
    • rows, cols: grid dimensions (default: 4×4)
    • timeout: inline in the URL (!timeout=30)
    • max_concurrent_processes computed automatically

Example:
    > python stress-test01.py

Output:
    Prints grid cell positions, DPI scaling factors, and queueing info.
    Opens a series of Chrome windows that auto‐close after the timeout.
"""
import subprocess
from screeninfo import get_monitors
import ctypes
from ctypes import wintypes
from concurrent.futures import ThreadPoolExecutor
import signal
import sys
import random

def get_working_area():
    """Retrieve the working area of the primary monitor, excluding the taskbar."""
    SPI_GETWORKAREA = 0x0030
    rect = wintypes.RECT()
    ctypes.windll.user32.SystemParametersInfoW(SPI_GETWORKAREA, 0, ctypes.byref(rect), 0)
    return rect.left, rect.top, rect.right, rect.bottom

def get_monitor_working_area(monitor):
    """Retrieve the working area of a specific monitor, excluding the taskbar."""
    monitor_rect = wintypes.RECT(
        monitor.x, monitor.y, monitor.x + monitor.width, monitor.y + monitor.height
    )
    hmonitor = ctypes.windll.user32.MonitorFromRect(
        ctypes.byref(monitor_rect), 2  # MONITOR_DEFAULTTONEAREST
    )

    class MONITORINFO(ctypes.Structure):
        _fields_ = [
            ("cbSize", wintypes.DWORD),
            ("rcMonitor", wintypes.RECT),
            ("rcWork", wintypes.RECT),
            ("dwFlags", wintypes.DWORD),
        ]

    monitor_info = MONITORINFO()
    monitor_info.cbSize = ctypes.sizeof(MONITORINFO)

    if not ctypes.windll.user32.GetMonitorInfoW(hmonitor, ctypes.byref(monitor_info)):
        raise ctypes.WinError()

    # Return the working area (rcWork) of the monitor
    return (
        monitor_info.rcWork.left,
        monitor_info.rcWork.top,
        monitor_info.rcWork.right,
        monitor_info.rcWork.bottom,
    )

def get_dpi_scaling(monitor):
    """Retrieve the DPI scaling factor for a specific monitor."""
    monitor_rect = wintypes.RECT(
        monitor.x, monitor.y, monitor.x + monitor.width, monitor.y + monitor.height
    )
    hmonitor = ctypes.windll.user32.MonitorFromRect(
        ctypes.byref(monitor_rect), 2  # MONITOR_DEFAULTTONEAREST
    )

    # Load the Shcore.dll library for DPI scaling
    shcore = ctypes.windll.shcore
    dpi_x = ctypes.c_uint()
    dpi_y = ctypes.c_uint()

    # MDT_EFFECTIVE_DPI = 0
    if shcore.GetDpiForMonitor(hmonitor, 0, ctypes.byref(dpi_x), ctypes.byref(dpi_y)) != 0:
        return 1.0  # Default scaling factor if DPI retrieval fails

    # Return the DPI scaling factor (96 DPI = 100%)
    return dpi_x.value / 96.0

def divide_monitor_into_grid(monitor, rows, cols, working_area=None):
    """Divide a monitor into a grid of rows x cols, accounting for the taskbar, monitor offsets, and DPI scaling."""
    grid = []

    # Retrieve the DPI scaling factor for the monitor
    dpi_scaling = get_dpi_scaling(monitor)
    print(f"DPI scaling for monitor {monitor.name}: {dpi_scaling}")

    # Use the working area if provided, otherwise use the full monitor dimensions
    if working_area:
        monitor_x, monitor_y, monitor_width, monitor_height = (
            int(working_area[0] / dpi_scaling),
            int(working_area[1] / dpi_scaling),
            int((working_area[2] - working_area[0]) / dpi_scaling),
            int((working_area[3] - working_area[1]) / dpi_scaling),
        )
    else:
        monitor_x, monitor_y, monitor_width, monitor_height = (
            int(monitor.x / dpi_scaling),
            int(monitor.y / dpi_scaling),
            int(monitor.width / dpi_scaling),
            int(monitor.height / dpi_scaling),
        )

    base_cell_width = monitor_width // cols
    base_cell_height = monitor_height // rows

    for row in range(rows):
        for col in range(cols):
            # Calculate the position of the grid cell relative to the monitor's space
            x = col * base_cell_width
            y = row * base_cell_height

            # Dynamically calculate the width and height for each cell
            cell_width = (
                base_cell_width if col < cols - 1 else monitor_width - (col * base_cell_width)
            )
            cell_height = (
                base_cell_height if row < rows - 1 else monitor_height - (row * base_cell_height)
            )

            grid.append((x, y, cell_width, cell_height))
            print(f"Grid cell (relative to monitor): x={x}, y={y}, width={cell_width}, height={cell_height}")
    return grid

def main():
    # Get all connected monitors
    monitors = get_monitors()
    rows, cols = 4, 4 # Define the grid size (e.g., 4x4)
    max_concurrent_processes = rows*cols * len(monitors)# Control the number of subprocesses running at one time
    # Shuffle the grid cells across all monitors for random order
    all_grid_cells = []
    for monitor_index, monitor in enumerate(monitors):
        working_area = get_monitor_working_area(monitor)
        grid = divide_monitor_into_grid(monitor, rows, cols, working_area)
        for x, y, w, h in grid:
            all_grid_cells.append((monitor_index, x, y, w, h))
    # random.shuffle(all_grid_cells)
    # Iterate over the shuffled grid cells and open Chrome windows
    try:
        with ThreadPoolExecutor(max_workers=max_concurrent_processes) as executor:
            futures = []
            for monitor_index, x, y, w, h in all_grid_cells:
                labelText = f"Grid({x},{y},{w},{h}), Monitor {monitor_index}"
                hexColor = f"{random.randint(0, 255):02x}{random.randint(0, 255):02x}{random.randint(0, 255):02x}"
                h_adjst = h-98  # Adjust height to account for chrome
                l = f"https://placehold.co/{w}x{h_adjst}/{hexColor}/FFF?text={labelText}"
                url = f"debugchrome:{l}!x={x}&!y={y}&!w={w}&!h={h}&!monitor={monitor_index}&!openwindow&!timeout=30"
                print(f"Queueing window at x={x}, y={y}, w={w}, h={h}, monitor={monitor_index}")
                futures.append(executor.submit(subprocess.run, ["debugchrome.exe", url]))

            # Wait for all futures to complete
            for future in futures:
                future.result()
    except KeyboardInterrupt:
        print("\nProgram interrupted. Exiting gracefully...doesn't.")
        # Shutdown the executor immediately
        executor.shutdown(wait=False)
        sys.exit(0)

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nProgram interrupted. Exiting gracefully...doesn't.")
        sys.exit(0)
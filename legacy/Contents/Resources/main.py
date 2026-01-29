#!/usr/bin/env python3
"""
People - Flask server with browser launch
"""

import os
import signal
import subprocess
import threading
import time
import webbrowser
from pathlib import Path

PORT = 5050
URL = f"http://127.0.0.1:{PORT}"
CHAT_DB = Path.home() / "Library" / "Messages" / "chat.db"


def kill_existing():
    """Kill any existing People app or process on our port."""
    my_pid = os.getpid()
    
    # Kill by port
    try:
        result = subprocess.run(
            ["lsof", "-ti", f":{PORT}"],
            capture_output=True,
            text=True
        )
        pids = result.stdout.strip().split("\n")
        for pid in pids:
            if pid and int(pid) != my_pid:
                os.kill(int(pid), signal.SIGKILL)
    except Exception:
        pass
    
    # Also kill any other People.app processes
    try:
        result = subprocess.run(
            ["pgrep", "-f", "People.app/Contents/MacOS/People"],
            capture_output=True,
            text=True
        )
        pids = result.stdout.strip().split("\n")
        for pid in pids:
            if pid and int(pid) != my_pid:
                os.kill(int(pid), signal.SIGKILL)
    except Exception:
        pass
    
    time.sleep(0.3)


def check_full_disk_access():
    """Check if we have Full Disk Access by trying to read chat.db."""
    try:
        with open(CHAT_DB, 'rb') as f:
            f.read(1)
        return True
    except (PermissionError, OSError):
        return False


def request_full_disk_access():
    """Show dialog and open System Settings for Full Disk Access."""
    # Use osascript to show a native-looking alert
    script = '''
    tell application "System Events"
        display alert "People would like to access your Messages" ¬
            message "People needs Full Disk Access to read your iMessage conversations. Click Open System Settings, add People to the list, then relaunch." ¬
            as warning ¬
            buttons {"Quit", "Open System Settings"} ¬
            default button "Open System Settings"
    end tell
    '''
    result = subprocess.run(
        ["osascript", "-e", script],
        capture_output=True,
        text=True
    )
    
    if "Open System Settings" in result.stdout:
        # Open System Settings to Full Disk Access
        subprocess.run([
            "open",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
        ])
    
    # Exit either way - user needs to grant access and relaunch
    exit(0)


if __name__ == "__main__":
    # Check Full Disk Access first
    if not check_full_disk_access():
        request_full_disk_access()
    
    # Import app only after permission check passes
    from app import app
    
    # Kill any existing People instances
    kill_existing()
    
    # Open browser after short delay
    def open_browser():
        time.sleep(0.5)
        webbrowser.open(URL)
    
    threading.Thread(target=open_browser, daemon=True).start()
    
    # Run Flask (blocks until quit)
    app.run(port=PORT, threaded=True, use_reloader=False)

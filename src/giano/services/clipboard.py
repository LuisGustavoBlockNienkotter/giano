"""System clipboard for the CLI (the TUI uses ``App.copy_to_clipboard``)."""

from __future__ import annotations

import shutil
import subprocess
import sys


def copy(text: str) -> bool:
    """Copy text to the clipboard using the platform's tool. Returns success."""
    try:
        if sys.platform == "win32":
            # clip.exe understands UTF-16LE with BOM, which keeps accents intact.
            data = b"\xff\xfe" + text.encode("utf-16-le")
            subprocess.run(["clip"], input=data, check=True)
            return True
        if sys.platform == "darwin":
            subprocess.run(["pbcopy"], input=text.encode("utf-8"), check=True)
            return True
        for cmd in (["wl-copy"], ["xclip", "-selection", "clipboard"], ["xsel", "-ib"]):
            if shutil.which(cmd[0]):
                subprocess.run(cmd, input=text.encode("utf-8"), check=True)
                return True
    except (OSError, subprocess.CalledProcessError):
        return False
    return False

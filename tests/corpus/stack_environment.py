"""Explicit child-process selectors and evidence from the native loader."""
import os
from pathlib import Path
import re

SELECTORS = {"LIBVA_DRIVERS_PATH", "LIBVA_DRIVER_NAME", "VK_ICD_FILENAMES"}


def environment(prefix=None, selectors=None, base=None):
    selectors = selectors or {}
    if set(selectors) - SELECTORS:
        raise ValueError("unsupported stack environment selector")
    env = dict(os.environ if base is None else base)
    for key in SELECTORS | {"VK_DRIVER_FILES", "LD_LIBRARY_PATH", "LD_PRELOAD", "LD_DEBUG", "DRI_PRIME", "MESA_VK_DEVICE_SELECT"}:
        env.pop(key, None)
    if prefix:
        root = Path(prefix).resolve(strict=True)
        env["PATH"] = str(root / "bin") + os.pathsep + env.get("PATH", "")
        env["LD_LIBRARY_PATH"] = str(root / "lib")
    for key, value in selectors.items():
        if not isinstance(value, str) or not value or "\0" in value:
            raise ValueError(f"invalid {key}")
        if key == "LIBVA_DRIVER_NAME":
            if value != "iHD":
                raise ValueError("this matrix qualifies Intel iHD only")
        else:
            if os.pathsep in value:
                raise ValueError(f"{key} must identify one isolated driver root/ICD")
            value = str(Path(value).resolve(strict=True))
        env[key] = value
    return env


def loader_modules(stderr):
    """Only modules whose initializers actually ran, not requested search paths."""
    return sorted({str(Path(path).resolve(strict=True))
                   for path in re.findall(r"calling init:\s*(/[^\n]+)", stderr)
                   if Path(path).is_file()})

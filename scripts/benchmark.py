"""Benchmark the prepared Windows CLI, including OS peak working-set memory."""
import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


class Memory(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
        (name, ctypes.c_size_t) for name in ["PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                                           "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
                                           "PagefileUsage", "PeakPagefileUsage"]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("audio", type=Path)
    parser.add_argument("--live", action="store_true", help="Use the same phrase segmentation as microphone mode")
    args = parser.parse_args()
    get_memory = ctypes.WinDLL("psapi").GetProcessMemoryInfo
    get_memory.argtypes = [wintypes.HANDLE, ctypes.POINTER(Memory), wintypes.DWORD]
    get_memory.restype = wintypes.BOOL
    started = time.perf_counter()
    process = subprocess.Popen([str(ROOT / "target/release/whistle-cli.exe"), str(args.audio.resolve()), "en"] +
                               (["--live"] if args.live else []), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               creationflags=subprocess.CREATE_NO_WINDOW)
    stdout, stderr = process.communicate(timeout=120)
    memory = Memory()
    memory.cb = ctypes.sizeof(memory)
    ok = get_memory(wintypes.HANDLE(int(process._handle)), ctypes.byref(memory), memory.cb)
    print(json.dumps({"audio": args.audio.name, "mode": "live phrases" if args.live else "file",
                      "exit_code": process.returncode, "wall_seconds": round(time.perf_counter() - started, 3),
                      "peak_working_set_mib": round(memory.PeakWorkingSetSize / 1024 ** 2, 1) if ok else None,
                      "transcript": stdout.decode("utf-8"), "metrics": stderr.decode("utf-8")}, indent=2))
    if process.returncode:
        raise SystemExit(process.returncode)


if __name__ == "__main__":
    main()

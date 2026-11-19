#!/usr/bin/env python3
"""Measure herdr responsiveness at the host PTY (no pixels, no terminal emulator).

Usage: scripts/mherdr_bench.py <test> <herdr-binary> [<herdr-binary> ...]

Tests:
  echo    keypress until the echoed key arrives (idle shell)
  switch  ctrl+b n (next tab) until the new tab's last frame arrives
  wheel   one mouse wheel step until its frame arrives
  fps     frames per second during continuous wheel scrolling (8 ms events)
  flood   time and herdr CPU for `seq 1 5000000`

Each binary runs in its own throwaway HOME/session at 120x40. Set
MAX_FPS=144 to add `[advanced] max_fps` to the generated config (ignored
by binaries without that option), N to change the sample count.
"""
import fcntl, os, pty, random, select, shutil, signal, struct, subprocess
import sys, tempfile, termios, time

ROWS, COLS = 40, 120
END = b"\x1b[?2026l"  # end of a synchronized-output frame
SESSION = "mherdr-bench"


def drain(fd, quiet, limit):
    end, data = time.monotonic() + limit, b""
    while time.monotonic() < end:
        if not select.select([fd], [], [], quiet)[0]:
            break
        try:
            data += os.read(fd, 1 << 20)
        except OSError:
            break
    if b"\x1b[6n" in data:
        os.write(fd, b"\x1b[1;1R")
    if b"\x1b[c" in data or b"\x1b[0c" in data:
        os.write(fd, b"\x1b[?62;22c")
    return data


class Herdr:
    def __init__(self, binary):
        self.binary = binary
        self.home = tempfile.mkdtemp(prefix="mherdr-bench-")
        h = self.home
        self.env = {
            "PATH": os.environ["PATH"], "TERM": "xterm-256color", "LANG": "C.UTF-8",
            "HOME": h, "SHELL": "/bin/bash", "XDG_CONFIG_HOME": f"{h}/config",
            "XDG_STATE_HOME": f"{h}/state", "XDG_RUNTIME_DIR": f"{h}/run",
            "HERDR_CONFIG_PATH": f"{h}/config/herdr/config.toml",
        }
        for d in ("config/herdr", "state", "run"):
            os.makedirs(f"{h}/{d}", mode=0o700)
        with open(self.env["HERDR_CONFIG_PATH"], "w") as f:
            f.write("onboarding = false\n")
            if os.environ.get("MAX_FPS"):
                f.write(f"[advanced]\nmax_fps = {os.environ['MAX_FPS']}\n")
        with open(f"{h}/.bashrc", "w") as f:
            f.write("PS1='$ '\n")
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execvpe(binary, [binary, "--session", SESSION], self.env)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        os.kill(self.pid, signal.SIGWINCH)
        end = time.monotonic() + 15
        while time.monotonic() < end and drain(self.fd, 1.0, 3.0):
            pass

    def cli(self, *args):
        subprocess.run([self.binary, "--session", SESSION, *args], env=self.env,
                       capture_output=True, timeout=20, check=True)

    def run(self, cmd, marker, timeout=60):
        """Type a command and wait until its output contains marker."""
        os.write(self.fd, f"{cmd}; echo {marker[:3]}''{marker[3:]}\r".encode())
        end, buf = time.monotonic() + timeout, b""
        while time.monotonic() < end and marker.encode() not in buf:
            if select.select([self.fd], [], [], 0.5)[0]:
                buf = (buf + os.read(self.fd, 1 << 20))[-4096:]
        drain(self.fd, 0.3, 3.0)

    def until_frame(self, timeout=0.5):
        """Seconds until the next frame end, or None."""
        t0, buf = time.perf_counter(), b""
        while END not in buf:
            if not select.select([self.fd], [], [], timeout)[0]:
                return None
            buf += os.read(self.fd, 1 << 20)
        return time.perf_counter() - t0

    def cpu_ticks(self):
        total = 0
        for entry in os.listdir("/proc"):
            try:
                with open(f"/proc/{entry}/environ", "rb") as f:
                    if f"HOME={self.home}".encode() not in f.read().split(b"\0"):
                        continue
                with open(f"/proc/{entry}/cmdline", "rb") as f:
                    if b"herdr" not in f.read().split(b"\0")[0]:
                        continue
                with open(f"/proc/{entry}/stat") as f:
                    fields = f.read().rsplit(")", 1)[1].split()
                total += int(fields[11]) + int(fields[12])
            except (OSError, ValueError, IndexError):
                pass
        return total

    def close(self):
        subprocess.run([self.binary, "--session", SESSION, "server", "stop"],
                       env=self.env, capture_output=True, timeout=20)
        try:
            os.kill(self.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        shutil.rmtree(self.home, ignore_errors=True)


def stats(ms):
    v = sorted(ms)
    q = lambda p: v[min(len(v) - 1, int(p * len(v)))]
    return f"n={len(v)} p50={q(.5):.2f} p90={q(.9):.2f} p99={q(.99):.2f} ms"


def echo(h, n):
    out = []
    for i in range(n):
        key = b"wz"[i % 2:i % 2 + 1]
        drain(h.fd, 0.05, 1.0)
        t0, buf = time.perf_counter(), b""
        os.write(h.fd, key)
        while key not in buf and select.select([h.fd], [], [], 1.0)[0]:
            buf += os.read(h.fd, 65536)
        if key in buf:
            out.append((time.perf_counter() - t0) * 1e3)
        if i % 20 == 19:
            os.write(h.fd, b"\x15")
        time.sleep(random.uniform(0.02, 0.06))
    return stats(out)


def switch(h, n):
    h.run("seq 1 3000 | sed 's/$/ AAA lorem ipsum dolor sit amet/'", "AAADONE")
    h.cli("tab", "create")
    drain(h.fd, 0.3, 2.0)
    h.run("seq 1 3000 | sed 's/$/ BBB lorem ipsum dolor sit amet/'", "BBBDONE")
    out = []
    for _ in range(n):
        os.write(h.fd, b"\x02")  # prefix ctrl+b
        drain(h.fd, 0.05, 0.5)
        t0, last = time.perf_counter(), None
        os.write(h.fd, b"n")
        while select.select([h.fd], [], [], 0.05)[0]:
            if END in os.read(h.fd, 1 << 20):
                last = time.perf_counter()
        if last:
            out.append((last - t0) * 1e3)
        time.sleep(random.uniform(0.05, 0.12))
    return stats(out)


def wheel(h, n):
    h.run("seq 1 50000", "FILLDONE")
    out = []
    for i in range(n):
        button = 64 if (i // 100) % 2 == 0 else 65  # 64 up, 65 down
        os.write(h.fd, f"\x1b[<{button};60;20M".encode())
        t = h.until_frame()
        if t is not None:
            out.append(t * 1e3)
        time.sleep(random.uniform(0.003, 0.02))
    return stats(out)


def fps(h, _n, seconds=4.0, interval=0.008):
    h.run("seq 1 20000", "FILLDONE")
    frames, sent, start = 0, 0, time.perf_counter()
    next_event = start
    while (now := time.perf_counter()) - start < seconds:
        if now >= next_event:
            button = 64 if (sent // 150) % 2 == 0 else 65
            os.write(h.fd, f"\x1b[<{button};60;20M".encode())
            sent, next_event = sent + 1, next_event + interval
        if select.select([h.fd], [], [], max(0.0, next_event - now))[0]:
            frames += os.read(h.fd, 1 << 20).count(END)
    return f"{frames / seconds:.0f} fps ({sent} wheel events)"


def flood(h, _n):
    hz = os.sysconf("SC_CLK_TCK")
    c0, t0 = h.cpu_ticks(), time.perf_counter()
    h.run("seq 1 5000000", "FLOODDONE", timeout=120)
    secs = time.perf_counter() - t0
    return f"{secs:.1f} s, herdr cpu {(h.cpu_ticks() - c0) / hz / secs * 100:.0f}%"


TESTS = {"echo": (echo, 200), "switch": (switch, 60), "wheel": (wheel, 400),
         "fps": (fps, 0), "flood": (flood, 0)}

if __name__ == "__main__":
    if len(sys.argv) < 3 or sys.argv[1] not in TESTS:
        sys.exit(__doc__)
    test, default_n = TESTS[sys.argv[1]]
    for binary in sys.argv[2:]:
        h = Herdr(os.path.abspath(binary))
        try:
            print(f"{sys.argv[1]:6s} {binary}: {test(h, int(os.environ.get('N', default_n)))}")
        finally:
            h.close()

#!/usr/bin/env python3
"""Archipelago gamepad→keyboard bridge (kiosk nodes).

Reads every attached game controller via evdev and mirrors it as a virtual
uinput KEYBOARD, so gamepad input works in every app — including cross-origin
iframes (IndeeHub, Jellyfin, …) where the web shell can never inject events.
The browser just sees arrow/Enter/Escape keys from a real-looking keyboard;
X autorepeat handles held directions. Design: docs/tv-input-iframe-apps.md.

Mapping (standard pad):
  D-pad / left stick  -> arrow keys
  A (BTN_SOUTH)       -> Enter        B (BTN_EAST)  -> Escape
  X (BTN_NORTH/WEST*) -> Space        Y             -> f (player fullscreen)
  LB (BTN_TL)         -> Shift+Tab    RB (BTN_TR)   -> Tab
  Start               -> Enter        Select        -> Escape

*Controllers disagree on NORTH/WEST for X/Y; both map to Space/f — either way
 one is play/pause and one is fullscreen, which is fine for a TV.

Pure stdlib (struct/fcntl/select) — no python3-evdev dependency on the node.
Runs as root (uinput + /dev/input need it); hotplug via 5s rescans.
"""

import fcntl
import os
import select
import struct
import time

# ---- kernel constants ------------------------------------------------------
EV_SYN, EV_KEY, EV_ABS = 0x00, 0x01, 0x03
SYN_REPORT = 0

KEY_ESC, KEY_TAB, KEY_ENTER, KEY_SPACE = 1, 15, 28, 57
KEY_LEFTSHIFT, KEY_F = 42, 33
KEY_UP, KEY_LEFT, KEY_RIGHT, KEY_DOWN = 103, 105, 106, 108

BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST = 0x130, 0x131, 0x133, 0x134
BTN_TL, BTN_TR, BTN_SELECT, BTN_START = 0x136, 0x137, 0x13A, 0x13B
BTN_DPAD_UP, BTN_DPAD_DOWN, BTN_DPAD_LEFT, BTN_DPAD_RIGHT = 0x220, 0x221, 0x222, 0x223

ABS_X, ABS_Y, ABS_HAT0X, ABS_HAT0Y = 0x00, 0x01, 0x10, 0x11

EVIOCGBIT_EV_KEY = 0x80604521  # EVIOCGBIT(EV_KEY, 96) — enough for BTN range
UI_SET_EVBIT, UI_SET_KEYBIT = 0x40045564, 0x40045565
UI_DEV_CREATE, UI_DEV_DESTROY = 0x5501, 0x5502

INPUT_EVENT = struct.Struct("llHHi")  # timeval sec/usec, type, code, value

BUTTON_MAP = {
    BTN_SOUTH: (KEY_ENTER,),
    BTN_EAST: (KEY_ESC,),
    BTN_NORTH: (KEY_SPACE,),
    BTN_WEST: (KEY_F,),
    BTN_TL: (KEY_LEFTSHIFT, KEY_TAB),
    BTN_TR: (KEY_TAB,),
    BTN_START: (KEY_ENTER,),
    BTN_SELECT: (KEY_ESC,),
    BTN_DPAD_UP: (KEY_UP,),
    BTN_DPAD_DOWN: (KEY_DOWN,),
    BTN_DPAD_LEFT: (KEY_LEFT,),
    BTN_DPAD_RIGHT: (KEY_RIGHT,),
}
EMITTED_KEYS = sorted({k for keys in BUTTON_MAP.values() for k in keys}
                      | {KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT})

STICK_THRESHOLD = 0.55  # fraction of full deflection before a stick "presses"


def is_gamepad(fd) -> bool:
    buf = bytearray(96)
    try:
        fcntl.ioctl(fd, EVIOCGBIT_EV_KEY, buf)
    except OSError:
        return False
    def has(code):
        return bool(buf[code // 8] & (1 << (code % 8)))
    return has(BTN_SOUTH) or has(BTN_START)


class VirtualKeyboard:
    def __init__(self):
        self.fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
        fcntl.ioctl(self.fd, UI_SET_EVBIT, EV_KEY)
        for key in EMITTED_KEYS:
            fcntl.ioctl(self.fd, UI_SET_KEYBIT, key)
        # Legacy uinput_user_dev setup struct: name[80] + input_id + ff_effects
        # + absmax/absmin/absfuzz/absflat (64 ints each) — works on every kernel.
        name = b"Archipelago Gamepad Keys"
        setup = name.ljust(80, b"\0") + struct.pack("HHHHi", 0x06, 0x1, 0x1, 1, 0)
        setup += b"\0" * (64 * 4 * 4)
        os.write(self.fd, setup)
        fcntl.ioctl(self.fd, UI_DEV_CREATE)

    def _emit(self, etype, code, value):
        os.write(self.fd, INPUT_EVENT.pack(0, 0, etype, code, value))

    def set_key(self, key, pressed):
        self._emit(EV_KEY, key, 1 if pressed else 0)
        self._emit(EV_SYN, SYN_REPORT, 0)

    def chord(self, keys, pressed):
        seq = keys if pressed else tuple(reversed(keys))
        for k in seq:
            self._emit(EV_KEY, k, 1 if pressed else 0)
        self._emit(EV_SYN, SYN_REPORT, 0)


class PadState:
    """Per-device axis state → synthetic arrow presses."""

    def __init__(self):
        self.axis_keys = {}  # axis -> currently-pressed arrow key (or None)
        self.abs_range = {}  # axis -> (min, max) for sticks

    def arrow_for(self, axis, value):
        if axis in (ABS_HAT0X, ABS_HAT0Y):
            if value < 0:
                return KEY_LEFT if axis == ABS_HAT0X else KEY_UP
            if value > 0:
                return KEY_RIGHT if axis == ABS_HAT0X else KEY_DOWN
            return None
        lo, hi = self.abs_range.get(axis, (-32768, 32767))
        span = (hi - lo) or 1
        norm = (2 * (value - lo) / span) - 1
        if norm <= -STICK_THRESHOLD:
            return KEY_LEFT if axis == ABS_X else KEY_UP
        if norm >= STICK_THRESHOLD:
            return KEY_RIGHT if axis == ABS_X else KEY_DOWN
        return None


def stick_range(fd, axis):
    # EVIOCGABS(axis): struct input_absinfo { value, min, max, fuzz, flat, res }
    buf = bytearray(24)
    try:
        fcntl.ioctl(fd, 0x80184540 + axis, buf)
        _, lo, hi = struct.unpack("iii", bytes(buf[:12]))
        if hi > lo:
            return (lo, hi)
    except OSError:
        pass
    return (-32768, 32767)


def main():
    os.system("modprobe uinput 2>/dev/null")
    kbd = VirtualKeyboard()
    pads = {}  # path -> (fd, PadState)
    last_scan = 0.0

    while True:
        now = time.monotonic()
        if now - last_scan > 5:
            last_scan = now
            try:
                names = sorted(os.listdir("/dev/input"))
            except FileNotFoundError:
                names = []
            for name in names:
                if not name.startswith("event"):
                    continue
                path = "/dev/input/" + name
                if path in pads:
                    continue
                try:
                    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
                except OSError:
                    continue
                if is_gamepad(fd):
                    state = PadState()
                    for axis in (ABS_X, ABS_Y):
                        state.abs_range[axis] = stick_range(fd, axis)
                    pads[path] = (fd, state)
                    print(f"gamepad attached: {path}", flush=True)
                else:
                    os.close(fd)

        if not pads:
            time.sleep(2)
            continue

        readable, _, _ = select.select([fd for fd, _ in pads.values()], [], [], 2.0)
        gone = []
        for path, (fd, state) in list(pads.items()):
            if fd not in readable:
                continue
            try:
                data = os.read(fd, INPUT_EVENT.size * 64)
            except OSError:
                gone.append(path)
                continue
            for off in range(0, len(data) - INPUT_EVENT.size + 1, INPUT_EVENT.size):
                _, _, etype, code, value = INPUT_EVENT.unpack_from(data, off)
                if etype == EV_KEY and code in BUTTON_MAP and value in (0, 1):
                    keys = BUTTON_MAP[code]
                    if len(keys) == 1:
                        kbd.set_key(keys[0], value == 1)
                    else:
                        kbd.chord(keys, value == 1)
                elif etype == EV_ABS and code in (ABS_X, ABS_Y, ABS_HAT0X, ABS_HAT0Y):
                    want = state.arrow_for(code, value)
                    held = state.axis_keys.get(code)
                    if want != held:
                        if held is not None:
                            kbd.set_key(held, False)
                        if want is not None:
                            kbd.set_key(want, True)
                        state.axis_keys[code] = want
        for path in gone:
            fd, state = pads.pop(path)
            for held in state.axis_keys.values():
                if held is not None:
                    kbd.set_key(held, False)
            try:
                os.close(fd)
            except OSError:
                pass
            print(f"gamepad detached: {path}", flush=True)


if __name__ == "__main__":
    main()

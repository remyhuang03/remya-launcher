#!/usr/bin/env python3
"""Probe the installed Apps sidecar and Launcher overlay on Hyprland."""

import argparse
import json
import subprocess
import time
from pathlib import Path


def output(*command, input=None):
    return subprocess.check_output(command, input=input, text=True, timeout=6)


def bus_call(method):
    return output(
        'busctl', '--user', 'call', 'org.freedesktop.DBus',
        '/org/freedesktop/DBus', 'org.freedesktop.DBus', method,
        's', 'top.remya.launcher',
    ).strip()


def launcher_layers(pid):
    monitors = json.loads(output('hyprctl', '-j', 'layers'))
    return {
        layer['namespace']
        for monitor in monitors.values()
        for level in monitor['levels'].values()
        for layer in level
        if layer.get('pid') == pid
    }


def check_overlay(executable):
    if bus_call('NameHasOwner') != 'b false':
        raise RuntimeError('Close the open Launcher before running this probe.')
    with subprocess.Popen([executable]) as launcher:
        try:
            deadline = time.monotonic() + 8
            while 'remya-launcher' not in launcher_layers(launcher.pid):
                if launcher.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError('The test Launcher overlay did not appear.')
                time.sleep(0.1)

            if bus_call('GetConnectionUnixProcessID') != f'u {launcher.pid}':
                raise RuntimeError('Another process owns the Launcher D-Bus name.')
            output(executable, '--toggle')
            if launcher.wait(timeout=5) != 0 or launcher_layers(launcher.pid):
                raise RuntimeError('The test Launcher did not close cleanly.')
        finally:
            if launcher.poll() is None:
                launcher.terminate()
                try:
                    launcher.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    launcher.kill()
                    launcher.wait(timeout=4)


def wait_for(predicate, message):
    deadline = time.monotonic() + 8
    while not predicate():
        if time.monotonic() >= deadline:
            raise RuntimeError(message)
        time.sleep(0.025)


def check_resident_keys(executable):
    if bus_call('NameHasOwner') != 'b false':
        raise RuntimeError('Quit the running Launcher before running this probe.')
    with subprocess.Popen([executable, '--background']) as launcher:
        try:
            wait_for(
                lambda: launcher.poll() is not None or bus_call('NameHasOwner') == 'b true',
                'Launcher did not register its D-Bus name.',
            )
            if (launcher.poll() is not None or
                    bus_call('GetConnectionUnixProcessID') != f'u {launcher.pid}'):
                raise RuntimeError('The keyboard probe requires Fast startup.')
            for _ in range(3):
                output(executable)
                wait_for(
                    lambda: 'remya-launcher' in launcher_layers(launcher.pid),
                    'The resident Launcher did not open.',
                )
                try:
                    output('ydotool', 'key', '1:1')
                    wait_for(
                        lambda: not launcher_layers(launcher.pid),
                        'The first Escape press did not close the resident Launcher.',
                    )
                finally:
                    # The release must happen after unmapping to reproduce the bug.
                    output('ydotool', 'key', '1:0')
                if launcher.poll() is not None:
                    raise RuntimeError('Launcher exited instead of staying resident.')
        finally:
            if launcher.poll() is None:
                launcher.terminate()
                launcher.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        'executable', nargs='?', type=Path, default=Path.home() / '.local/bin/remya_launcher',
    )
    parser.add_argument('--resident-keys', action='store_true', help=(
        'verify that each resident open closes on the first Escape, using ydotool'
    ))
    args = parser.parse_args()
    executable = args.executable.resolve(strict=True)
    apps = executable.with_name('remya-launcher-apps')
    reply = json.loads(output(apps, input='{"id":1,"op":"windows"}\n'))
    if reply.get('id') != 1 or not isinstance(reply.get('data', {}).get('windows'), list):
        raise RuntimeError('The installed Apps sidecar did not return the Windows list.')
    print('Installed Apps sidecar returned the Windows list.')
    if args.resident_keys:
        check_resident_keys(executable)
        print('Three resident opens each closed on the first Escape.')
    else:
        check_overlay(executable)
        print('Launcher overlay mapped and closed without leftover layers.')


if __name__ == '__main__':
    main()

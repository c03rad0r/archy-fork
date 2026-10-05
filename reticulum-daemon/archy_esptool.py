"""Packaged Espressif CLI: no system Python or first-use package install required."""
import sys
import esptool
from esptool.loader import StubFlasher


def self_test():
    # Debian's stripped package omitted this blob; validate the actual bundled
    # resources before accepting a flash job, without opening a serial port.
    stub = StubFlasher('ESP32-S3')
    if not stub.text or not stub.text_start or not stub.entry:
        raise RuntimeError('ESP32-S3 flashing stub is incomplete')
    print(f'archy-esptool {esptool.__version__}: ESP32-S3 stub ready')


if __name__ == '__main__':
    if sys.argv[1:] == ['--archy-self-test']:
        self_test()
    else:
        esptool._main()

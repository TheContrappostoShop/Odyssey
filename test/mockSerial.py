#!/usr/bin/env python
import os, pty, serial, time

master, slave = pty.openpty()
s_name = os.ttyname(slave)
m_name = os.ttyname(master)
print(f"Virtual Serial Port: {s_name}")

while True:
    # To read from the device
    time.sleep(1)
    commands = os.read(master,1000).decode('utf-8')

    if commands:
        print(f"Received: {commands}")

    if "g0" in commands.lower() or "g1" in commands.lower():
        os.write(master, bytes('Z_move_comp\r\n', 'utf-8'))

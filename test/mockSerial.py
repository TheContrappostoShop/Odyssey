#!/usr/bin/env python3
import os, pty, serial, time

master, slave = pty.openpty()
s_name = os.ttyname(slave)
m_name = os.ttyname(master)
print(f"Virtual Serial Port: {s_name}")

status_counter = 0

while True:
    # To read from the device
    time.sleep(1)
    commands = os.read(master,1000).decode('utf-8')

    if commands:
        print(f"Received: {commands}")

    if "g0" in commands.lower() or "g1" in commands.lower():
        os.write(master, bytes('Z_move_comp\r\n', 'utf-8'))
    elif "status" in commands.lower():
        if status_counter<10:
            status_counter += 1
            os.write(master, bytes('Klipper state: Disconnect\r\n', 'utf-8'))
        else:
            status_counter = 0
            os.write(master, bytes('Klipper state: Ready\r\n', 'utf-8'))

#!/usr/bin/env python3
import websocket
try:
    import thread
except ImportError:
    import _thread as thread
import time
import sys

class COLORS:
    HEADER = '\033[95m'
    OKBLUE = '\033[94m'
    OKCYAN = '\033[96m'
    OKGREEN = '\033[92m'
    WARNING = '\033[93m'
    FAIL = '\033[91m'
    ENDC = '\033[0m'
    BOLD = '\033[1m'
    UNDERLINE = '\033[4m'

def on_open(ws):
    print("connection is open")
    with open(sys.argv[1], 'rb') as file:
        data = file.read()
    def run(ws, data):
        time.sleep(1)
        ws.send(data, websocket.ABNF.OPCODE_BINARY)
        print("file sent, waiting for response")
    thread.start_new_thread(run, (ws, data))

def on_error(ws, error):
    print(COLORS.WARNING + error + COLORS.ENDC)

def on_message(ws, msg):
    if msg and msg[0] == '[':
        print(COLORS.OKGREEN + COLORS.BOLD + msg + COLORS.ENDC)
    elif msg == "Finished":
        print(COLORS.OKBLUE + COLORS.BOLD + msg + COLORS.ENDC)
        ws.close()
    else:
        print(msg)


if __name__ == "__main__":
    ws = websocket.WebSocketApp(sys.argv[2], on_message = on_message, on_error = on_error)
    ws.on_open = on_open
    ws.run_forever()

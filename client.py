import websocket
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


ws = websocket.WebSocket()
ws.connect(sys.argv[2])
with open(sys.argv[1], 'rb') as file:
    data = file.read()

ws.send_binary(data)

while True:
    result = ws.recv()
    if result and result[0] == '[':
        print(COLORS.OKGREEN + COLORS.BOLD + result + COLORS.ENDC)
    elif result == "Finished":
        print(COLORS.OKBLUE + COLORS.BOLD + result + COLORS.ENDC)
        break
    else:
        print(result)

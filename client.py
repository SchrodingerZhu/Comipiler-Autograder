import websocket
import sys
ws = websocket.WebSocket()
ws.connect("ws://127.0.0.1:8080")
with open(sys.argv[1], 'rb') as file:
    data = file.read()
    
ws.send_binary(data)

while True:
    result = ws.recv()
    print(result)
    if result == "Finished":
        break
    

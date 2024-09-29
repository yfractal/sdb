import os

fifo_path = '/tmp/start_symbolizer'

if not os.path.exists(fifo_path):
    os.mkfifo(fifo_path)

with open(fifo_path, 'r') as fifo:
    print("Waiting for data...")
    while True:
        data = fifo.read().strip()
        if len(data) == 0:
            break
        print(f"Received: {data}")

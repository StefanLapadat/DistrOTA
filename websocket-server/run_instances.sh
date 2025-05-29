#!/bin/bash

cargo build --release

PIDS=()

cleanup() {
    echo "Cleaning up child processes..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null
    done
}

trap cleanup SIGINT SIGTERM EXIT

for i in {0..2}
do
  port=$((8080 + i))
  address="127.0.0.1:$port"
  log_file="./log/log$i.txt"
  echo "Starting instance $i on port $address"
  ./target/release/websocket-server "$address" "$i" >"$log_file">&1 &

  PIDS+=($!)
done

wait

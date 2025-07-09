#!/bin/bash

duration=180
length=64

log_file="log_default.txt"
interface="eno1"

echo "[1/1] Launching benchmark at $(date)" | tee "$log_file"
start_time=$(date '+%Y-%m-%d %H:%M:%S')
echo "Start Time: $start_time" | tee -a "$log_file"
echo "Duration: $duration"

# 백그라운드에서 실행하고 로그 저장	
RUST_BACKTRACE=1 cargo run --release -- \
	--address "192.168.1.121:8050" \
	--number 16384 \
	--duration "$duration" \
	--length "$length" >> "$log_file" 2>&1

sleep 1

end_time=$(date '+%Y-%m-%d %H:%M:%S')
echo "End Time: $end_time" | tee -a "$log_file"

	
echo "✅ benchmark completed."

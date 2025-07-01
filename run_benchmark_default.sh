#!/bin/bash

duration=180
length=64

log_file="log_default.txt"
net_log_file="ifstat_log_default.txt"
interface="eno1"

echo "[1/1] Launching benchmark at $(date)" | tee "$log_file"
start_time=$(date '+%Y-%m-%d %H:%M:%S')
echo "Start Time: $start_time" | tee -a "$log_file"
echo "Duration: $duration"

# Start ifstat logging in background
echo "▶️ Starting ifstat logging..." | tee -a "$log_file"
ifstat -i "$interface" 1 > "$net_log_file" &
ifstat_pid=$!

# 백그라운드에서 실행하고 로그 저장	
RUST_BACKTRACE=1 cargo run --release -- \
	--address "192.168.1.121:8050" \
	--number 8192 \
	--duration "$duration" \
	--length "$length" >> "$log_file" 2>&1

# Kill ifstat after duration
sleep 1
kill "$ifstat_pid"
echo "⏹ ifstat stopped." | tee -a "$log_file"

end_time=$(date '+%Y-%m-%d %H:%M:%S')
echo "End Time: $end_time" | tee -a "$log_file"

	
echo "✅ benchmark completed."

echo -e "\n=== ifstat log ===" >> "$log_file"
cat "$net_log_file" >> "$log_file"

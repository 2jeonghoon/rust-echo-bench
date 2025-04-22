#!/bin/bash

duration=300

log_file="log_${i}.txt"
echo "[$i/10] Launching benchmark at $(date)" | tee "$log_file"
start_time=$(date '+%Y-%m-%d %H:%M:%S')
echo "Start Time: $start_time" | tee -a "$log_file"

# 백그라운드에서 실행하고 로그 저장	
cargo run --release -- \
	--address "117.16.44.111:8050" \
	--number 1000 \
	--duration "$duration" \
	--length 32 >> "$log_file" 2>&1

echo "Benchmark $i started in background (PID $!)"
		
echo "✅ All 10 benchmarks launched in background."

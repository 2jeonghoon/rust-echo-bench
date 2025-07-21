BASE_PORT=8050
NUM_ROUNDS=16
CLIENTS_PER_ROUND=128
INTERVAL=60
LENGTH=1
LOG_BASE="log_default"
ADDRESS_PREFIX="192.168.1.121"

start_time_global=$(date '+%Y-%m-%d %H:%M:%S')
echo "[🚀 Benchmark 시작] $start_time_global"
echo "총 클라이언트 수: $((NUM_ROUNDS * CLIENTS_PER_ROUND))"

# 클라이언트 PID 추적용 배열
declare -a CLIENT_PIDS

for ((i=0; i<NUM_ROUNDS; i++)); do
    PORT=$((BASE_PORT + i))
	LOG_FILE="${LOG_BASE}_port${PORT}.txt"
	echo ""
	echo "[$((i+1))/$NUM_ROUNDS] ⏱️ $(date '+%H:%M:%S') - 포트 $PORT에서 클라이언트 실행 중..."
	
	# 클라이언트 실행 (지속 실행)
	cargo run --release -- \
		--address "$ADDRESS_PREFIX:$PORT" \
		--number "$CLIENTS_PER_ROUND" \
		--duration $((60*(NUM_ROUNDS + 1 - i))) \
		--length "$LENGTH" >> "$LOG_FILE" 2>&1 &
	CLIENT_PIDS+=($!)
	sleep $INTERVAL
done

echo ""
echo "🕒 마지막 클라이언트 실행 완료: $(date '+%H:%M:%S')"
echo "🕓 120초 후 전체 종료를 시작합니다..."
sleep 120
echo ""
echo "🛑 전체 실험 종료 중..."

end_time_global=$(date '+%Y-%m-%d %H:%M:%S')
echo ""
echo "✅ 실험 종료 완료. 시작: $start_time_global / 종료: $end_time_global"

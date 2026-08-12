#!/usr/bin/env bash
# Uptime Monitor for REL-05
# Runs every 5 minutes via cron, records metrics to a CSV file.
# Install: */5 * * * * /opt/archipelago/scripts/uptime-monitor.sh
#
# Tracks: timestamp, http_status, response_time_ms, cpu_percent,
#         mem_used_mb, mem_total_mb, disk_used_gb, disk_total_gb,
#         container_count, uptime_secs, restart_count

set -euo pipefail

LOG_DIR="/var/lib/archipelago/uptime-monitor"
LOG_FILE="$LOG_DIR/metrics.csv"
RESTART_FILE="$LOG_DIR/restart-count"
BACKEND_URL="http://localhost:5678/health"
RPC_URL="http://localhost:5678/rpc/v1"

mkdir -p "$LOG_DIR"

# Write CSV header if file doesn't exist
if [ ! -f "$LOG_FILE" ]; then
    echo "timestamp,http_status,response_ms,cpu_percent,mem_used_mb,mem_total_mb,disk_used_gb,disk_total_gb,containers,uptime_secs,restart_count" > "$LOG_FILE"
fi

# Track restart count
if [ ! -f "$RESTART_FILE" ]; then
    echo "0" > "$RESTART_FILE"
fi
RESTART_COUNT=$(cat "$RESTART_FILE" 2>/dev/null || echo "0")

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Check HTTP health
HTTP_START=$(date +%s%N)
HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$BACKEND_URL" 2>/dev/null || echo "000")
HTTP_END=$(date +%s%N)
RESPONSE_MS=$(( (HTTP_END - HTTP_START) / 1000000 ))

# Authenticate for RPC access
curl -s -c /tmp/uptime-cookies --max-time 5 -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"method":"auth.login","params":{"password":"password123"}}' >/dev/null 2>&1
CSRF=$(grep csrf_token /tmp/uptime-cookies 2>/dev/null | awk '{print $NF}')

# Get system stats from RPC
STATS=$(curl -s --max-time 10 -b /tmp/uptime-cookies \
    -H "Content-Type: application/json" \
    -H "X-CSRF-Token: $CSRF" \
    -X POST "$RPC_URL" \
    -d '{"method":"system.stats"}' 2>/dev/null || echo '{"result":{}}')

CPU=$(echo "$STATS" | python3 -c "import sys,json; d=json.load(sys.stdin).get('result',{}); print(d.get('cpu_usage_percent',0))" 2>/dev/null || echo "0")
MEM_USED=$(echo "$STATS" | python3 -c "import sys,json; d=json.load(sys.stdin).get('result',{}); print(round(d.get('mem_used_bytes',0)/1048576))" 2>/dev/null || echo "0")
MEM_TOTAL=$(echo "$STATS" | python3 -c "import sys,json; d=json.load(sys.stdin).get('result',{}); print(round(d.get('mem_total_bytes',0)/1048576))" 2>/dev/null || echo "0")
DISK_USED=$(echo "$STATS" | python3 -c "import sys,json; d=json.load(sys.stdin).get('result',{}); print(round(d.get('disk_used_bytes',0)/1073741824,1))" 2>/dev/null || echo "0")
DISK_TOTAL=$(echo "$STATS" | python3 -c "import sys,json; d=json.load(sys.stdin).get('result',{}); print(round(d.get('disk_total_bytes',0)/1073741824,1))" 2>/dev/null || echo "0")
UPTIME=$(echo "$STATS" | python3 -c "import sys,json; d=json.load(sys.stdin).get('result',{}); print(d.get('uptime_secs',0))" 2>/dev/null || echo "0")

# Count running containers
CONTAINERS=$(podman ps --format "{{.Names}}" 2>/dev/null | wc -l || echo "0")

# Detect restart (uptime < 300s = likely just restarted)
if [ "$UPTIME" -lt 300 ] 2>/dev/null; then
    # Check if we already counted this restart
    LAST_UPTIME_FILE="$LOG_DIR/last-uptime"
    LAST_UPTIME=$(cat "$LAST_UPTIME_FILE" 2>/dev/null || echo "99999")
    if [ "$LAST_UPTIME" -gt 300 ] 2>/dev/null; then
        RESTART_COUNT=$((RESTART_COUNT + 1))
        echo "$RESTART_COUNT" > "$RESTART_FILE"
    fi
    echo "$UPTIME" > "$LAST_UPTIME_FILE"
else
    echo "$UPTIME" > "$LOG_DIR/last-uptime"
fi

# Append metrics
echo "$TIMESTAMP,$HTTP_STATUS,$RESPONSE_MS,$CPU,$MEM_USED,$MEM_TOTAL,$DISK_USED,$DISK_TOTAL,$CONTAINERS,$UPTIME,$RESTART_COUNT" >> "$LOG_FILE"

# Generate summary report
TOTAL_CHECKS=$(wc -l < "$LOG_FILE")
TOTAL_CHECKS=$((TOTAL_CHECKS - 1))  # exclude header
if [ "$TOTAL_CHECKS" -gt 0 ]; then
    OK_CHECKS=$(grep -c ",200," "$LOG_FILE" || echo "0")
    UPTIME_PCT=$(python3 -c "print(round($OK_CHECKS / $TOTAL_CHECKS * 100, 3))" 2>/dev/null || echo "0")

    cat > "$LOG_DIR/summary.json" << EOF
{
    "start": "$(head -2 "$LOG_FILE" | tail -1 | cut -d',' -f1)",
    "last_check": "$TIMESTAMP",
    "total_checks": $TOTAL_CHECKS,
    "ok_checks": $OK_CHECKS,
    "uptime_percent": $UPTIME_PCT,
    "restart_count": $RESTART_COUNT,
    "current_status": "$HTTP_STATUS"
}
EOF
fi

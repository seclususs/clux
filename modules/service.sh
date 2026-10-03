#!/system/bin/sh

MODDIR="${0%/*}"
DAEMON_BIN="$MODDIR/system/bin/qos_daemon"
PID_FILE="$MODDIR/daemon.pid"

while [ "$(getprop sys.boot_completed)" != "1" ]; do
    sleep 1
done

if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        exit 0
    fi
    rm -f "$PID_FILE"
fi

nohup "$DAEMON_BIN" > /dev/null 2>&1 &
echo $! > "$PID_FILE"

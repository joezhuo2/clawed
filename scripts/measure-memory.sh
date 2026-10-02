#!/bin/sh
# macOS counterpart of measure-memory.ps1. Sums resident memory of the islet
# process and the WebKit XPC processes (WebContent, Networking, GPU) started
# after it. WebKit processes are children of launchd, not of islet, so this
# matches by start time: don't open Safari or other WebKit apps while
# measuring.
# Usage: sh scripts/measure-memory.sh [label]
label=${1:-sample}
pid=$(pgrep -x islet | head -n 1)
[ -n "$pid" ] || { echo "islet is not running"; exit 1; }

epoch() { date -j -f "%a %b %d %T %Y" "$1" +%s 2>/dev/null; }
start=$(epoch "$(ps -o lstart= -p "$pid")")

pids="$pid"
for p in $(pgrep -f 'com\.apple\.WebKit\.(WebContent|Networking|GPU)'); do
  s=$(epoch "$(ps -o lstart= -p "$p")")
  [ -n "$s" ] && [ "$s" -ge "$start" ] && pids="$pids $p"
done

list=$(echo "$pids" | tr ' ' ',')
rss_kb=$(ps -o rss= -p "$list" | awk '{ s += $1 } END { print s }')
backend_kb=$(ps -o rss= -p "$pid" | tr -d ' ')
count=$(echo "$pids" | wc -w | tr -d ' ')
printf '%-14s processes=%-2s rss=%6.1f MB (backend alone: %.1f MB)\n' \
  "$label" "$count" "$(echo "$rss_kb / 1024" | bc -l)" "$(echo "$backend_kb / 1024" | bc -l)"
# Physical footprint (closest to Windows private bytes), when available.
command -v footprint >/dev/null && for p in $pids; do footprint -p "$p" 2>/dev/null | grep -i 'phys_footprint:' | sed "s/^/  pid $p /"; done
exit 0

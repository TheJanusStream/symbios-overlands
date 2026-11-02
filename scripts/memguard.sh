#!/bin/sh
# memguard.sh FLOOR_GIB CMD... - run CMD in its own process group, and kill the group
# if MemAvailable falls under FLOOR_GIB, so a runaway build cannot starve the desktop.
# A TERM takes the group down too: in its own session, CMD is out of anything else's reach.
floor_kib=$(( $1 * 1024 * 1024 )); shift
setsid "$@" &
pid=$!
down() {
    kill -TERM -"$pid" 2>/dev/null; sleep 2; kill -KILL -"$pid" 2>/dev/null
}
trap 'down; exit 143' TERM
while kill -0 "$pid" 2>/dev/null; do
    avail=$(awk '/MemAvailable/ {print $2}' /proc/meminfo)
    if [ "$avail" -lt "$floor_kib" ]; then
        echo "MEMGUARD: MemAvailable ${avail} kB under the floor - killing group $pid" >&2
        down
        exit 137
    fi
    sleep 1
done
wait "$pid"

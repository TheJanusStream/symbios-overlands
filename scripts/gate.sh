#!/usr/bin/env bash
# The gate of docs/building.md, every line of it, in less time.
#
#   scripts/gate.sh                run what the tree has not yet passed
#   scripts/gate.sh --patched      the same against ../symbios-avatar and
#                                  ../bevy_symbios_avatar, Cargo.lock put back after.
#                                  All but `cargo deny`, which cannot see a patch and
#                                  would pin the lockfile back on the registry
#   scripts/gate.sh --worker       also build the worker and check its deploy budget
#   scripts/gate.sh --force        run every step, passed or not
#   scripts/gate.sh --published    after a bump to crates that have just been published:
#                                  if they hold, file for file, what --patched passed on
#                                  with this tree, the checks without the tests and the
#                                  doc gate, which have run on that very source. Else all.
#
# It runs the same commands as the list in docs/building.md. Two things differ, and
# neither drops a check:
#
# - Steps that share no build lock run side by side, in lanes: tests, then the
#   debug-profile checks, then the wasm checks.
# - A step that passed is not run again while everything that can decide it is the
#   same: every file git would see, Cargo.lock, the toolchain, the command, the
#   flags and the siblings under --patched. The record lives in target/gate/, so
#   losing target/ only costs the time. `cargo deny` always runs, as what decides it is
#   an advisory database that moves overnight.
#
# It says `gate: passed` only when every step it owes has reported a pass and nothing it
# read has changed under it: a lane that dies, or an edit mid-run, reads as a failure.
#
# GATE_JOBS (24), GATE_MEM_FLOOR (8 GiB) and GATE_SIBLINGS (..) tune it.
set -u
# What `step` reads of its callers here: from outside, a record would be keyed on it.
unset STEP_PRINT STEP_ALWAYS
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
jobs=${GATE_JOBS:-24}
floor=${GATE_MEM_FLOOR:-8}
patched=0 force=0 worker=0 published=0
for arg in "$@"; do
    case $arg in
        --patched) patched=1 ;;
        --force) force=1 ;;
        --worker) worker=1 ;;
        --published) published=1 ;;
        *) sed -n '2,/^[^#]/p' "$0" | sed '$d'; exit 2 ;;
    esac
done
if [ "$patched" = 1 ] && [ "$published" = 1 ]; then
    echo "gate: --patched tests the trees, --published what came of them: one at a time" >&2
    exit 2
fi
gate=target/gate
logs=$gate/logs
passed=$gate/passed
mkdir -p "$logs" "$passed" || exit 2
results=$(mktemp -d) || exit 2
trap 'rm -rf "$results"' EXIT
touch "$results/report"
started=$(date +%s)
parent=${GATE_SIBLINGS:-..}
siblings=("$parent/symbios-avatar" "$parent/bevy_symbios_avatar")

# One run at a time, as a second would read the first's lockfile. A step left behind by
# a run that was killed holds this too.
exec 9> "$gate/lock"
if ! flock -n 9; then
    echo "gate: another run, or a step one left behind, is still going" >&2
    exit 2
fi

# Where --patched keeps the real lockfile, with its time: build.rs watches Cargo.lock, so
# a new time would recompile. Out of the temporary directory, so that a run killed
# outright leaves it for the next one to put back.
kept=$gate/Cargo.lock.kept
put_back_lock() {
    if [ -e "$kept" ]; then
        mv -f "$kept" Cargo.lock
    fi
}
put_back_lock
trap 'put_back_lock; rm -rf "$results"' EXIT

# Stopped: the steps first, or one could write the lockfile after it was put back.
stopped() {
    trap '' INT TERM
    kill $(jobs -p) 2>/dev/null
    if [ -e "$results/guards" ]; then
        local guard
        while read -r guard; do kill "$guard" 2>/dev/null; done < "$results/guards"
        while read -r guard; do
            while kill -0 "$guard" 2>/dev/null; do sleep 0.2; done
        done < "$results/guards"
    fi
    echo "gate: stopped" >&2
    exit 130
}
trap stopped INT TERM

# What every file in `dir` that git would see holds, as a list of names and hashes. In a
# fixed order: ls-files moves a file when it is staged, which changes nothing it holds.
tree_of() (
    cd "$1" || exit 1
    git ls-files -zco --exclude-standard | LC_ALL=C sort -zu | while IFS= read -r -d '' file; do
        [ -f "$file" ] && printf '%s\n' "$file"
    done > "$results/list"
    paste "$results/list" <(git hash-object --stdin-paths < "$results/list")
)

# A directory git does not hold would hash as empty, and leave every key unmoved by an edit.
is_checkout() {
    [ -e "$1/.git" ]
}

# The tools and flags a step's outcome depends on.
tools_of() {
    rustc -vV
    cargo -V
    cargo nextest --version 2>/dev/null
    env | grep -E '^(RUSTFLAGS|RUSTDOCFLAGS|RUSTC_[A-Z0-9_]*|CARGO_[A-Z0-9_]*|[A-Z0-9_]*BLESS[A-Z0-9_]*)=' | sort
    [ -f ~/.cargo/config.toml ] && sha256sum ~/.cargo/config.toml
}

# A manifest without what a bump moves: the versions it asks of the avatar crates.
asks_of() {
    sed -E 's/^((symbios-avatar|bevy_symbios_avatar) = (\{ version = )?)"[^"]*"/\1/' "$1"
}

# What --patched passed on, without what a bump is allowed to move: those versions, each
# sibling's own, and the lockfile's two entries that say where the crates came from.
tested_key() {
    {
        tree_of . | grep -vE '^(crates/gen-jobs/)?Cargo\.toml'$'\t'
        asks_of Cargo.toml
        asks_of crates/gen-jobs/Cargo.toml
        awk -v RS= -v ORS='\n\n' '!/\nname = "(symbios-avatar|bevy_symbios_avatar)"\n/' Cargo.lock
        for sibling in "${siblings[@]}"; do
            tree_of "$sibling" | grep -vE '^Cargo\.toml'$'\t'
            asks_of "$sibling/Cargo.toml" | sed '/^version = /d'
        done
        tools_of
    } | sha256sum | cut -c1-32
}

# Whether the crates Cargo.lock names hold, file for file, what the siblings here hold.
# A file only a sibling has is one the package left out: `cargo publish` built without it.
matches_siblings() {
    local sibling name version found
    for sibling in "${siblings[@]}"; do
        name=${sibling##*/}
        if ! is_checkout "$sibling"; then
            echo "gate: $sibling is not a git checkout: the whole gate"
            return 1
        fi
        version=$(grep -A1 "^name = \"$name\"$" Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p')
        found=$(ls -d ~/.cargo/registry/src/*/"$name-$version" 2>/dev/null | head -1)
        if [ -z "$found" ] || ! cmp -s "$found/Cargo.toml.orig" "$sibling/Cargo.toml" \
            || diff -rq --exclude='.cargo*' --exclude=Cargo.toml --exclude=Cargo.toml.orig \
                --exclude=Cargo.lock "$found" "$sibling" | grep -qv "^Only in $sibling"; then
            echo "gate: $name $version is not the source here: the whole gate"
            return 1
        fi
    done
}

[ -f Cargo.lock ] || cargo generate-lockfile > "$logs/lockfile.log" 2>&1

only_checks=0
if [ "$published" = 1 ] && [ "$force" = 0 ]; then
    cargo fetch > "$logs/fetch.log" 2>&1
    if matches_siblings; then
        if [ -e "$passed/published-$(tested_key)" ]; then
            only_checks=1
            echo "gate: --patched passed on this very source: no tests and no doc gate here"
        else
            echo "gate: no passed --patched run of this tree: the whole gate"
        fi
    fi
fi

config=()
if [ "$patched" = 1 ]; then
    for sibling in "${siblings[@]}"; do
        if ! is_checkout "$sibling"; then
            echo "gate: $sibling is not a git checkout, so no key could say what was tested" >&2
            exit 2
        fi
    done
    config=(--config "patch.crates-io.symbios-avatar.path=\"$parent/symbios-avatar\""
            --config "patch.crates-io.bevy_symbios_avatar.path=\"$parent/bevy_symbios_avatar\"")
    cp -p Cargo.lock "$kept" || exit 2
    cargo update -p symbios-avatar -p bevy_symbios_avatar "${config[@]}" > "$logs/update.log" 2>&1
    # A patch whose version does not match is silently ignored (CLAUDE.md, rule 3).
    for crate in symbios-avatar bevy_symbios_avatar; do
        if ! cargo tree -p "$crate" --depth 0 "${config[@]}" 2>/dev/null | head -1 | grep -q '(/'; then
            echo "gate: the patch for $crate is not in the build: read Cargo.lock" >&2
            exit 2
        fi
    done
fi

print_of() {
    {
        tree_of .
        sha256sum Cargo.lock
        tools_of
        if [ "$patched" = 1 ]; then
            for sibling in "${siblings[@]}"; do tree_of "$sibling"; done
        fi
    } | sha256sum | cut -c1-32
}
fingerprint=$(print_of)

# Taken now: what a --patched run vouches for is what it started on.
if [ "$patched" = 1 ]; then
    tested=$(tested_key)
fi

# Every step this run owes: a lane that dies takes its steps with it, and a step that
# never reported reads as a failure.
owed=(fmt clippy wasm webgpu)
[ "$patched" = 0 ] && owed+=(deny)
[ "$only_checks" = 0 ] && owed+=(build nextest lib doctest doc)
[ "$worker" = 1 ] && owed+=(worker size)
[ -f scripts/gate_test.sh ] && owed+=(selftest)

# step NAME COMMAND...: the guard, the record, one line of report. Under the memory
# guard, and in the background so that a stopped run can reach it.
step() {
    local name=$1; shift
    local record=$passed/$name-${STEP_PRINT:-$fingerprint}-$(printf '%s' "$*" | sha256sum | cut -c1-12)
    if [ "$force" = 0 ] && [ "${STEP_ALWAYS:-0}" = 0 ] && [ -e "$record" ]; then
        echo "$name: passed already" >> "$results/report"
        touch "$results/ok-$name"
        return 0
    fi
    local begun=$(date +%s) guard code
    sh scripts/memguard.sh "$floor" env CARGO_BUILD_JOBS="$jobs" "$@" > "$logs/$name.log" 2>&1 &
    guard=$!
    echo "$guard" >> "$results/guards"
    wait "$guard"
    code=$?
    local took=$(( $(date +%s) - begun ))
    if [ "$code" = 0 ]; then
        touch "$record" "$results/ok-$name"
        echo "$record" >> "$results/records"
        echo "$name: passed in $took s" >> "$results/report"
    else
        echo "$name: FAILED with $code in $took s, see $logs/$name.log" >> "$results/report"
        touch "$results/failed"
    fi
    return "$code"
}

check_worker() {
    local dist=$gate/dist
    mkdir -p "$dist"
    wasm-bindgen --out-dir "$dist" --target web --out-name gen-worker \
        target/wasm32-unknown-unknown/release/gen-worker.wasm || return 1
    local wasm=$dist/gen-worker_bg.wasm raw gz budget
    raw=$(stat -c %s "$wasm")
    gz=$(gzip -9c "$wasm" | wc -c)
    budget=$(awk '/check .*gen-worker_bg.wasm/ {print $3, $4}' .github/workflows/deploy.yml)
    echo "worker: raw $raw gzip $gz, budget ${budget% *} ${budget#* }"
    [ "$raw" -le "${budget% *}" ] && [ "$gz" -le "${budget#* }" ]
}

step fmt cargo fmt --all -- --check
if [ "$patched" = 1 ]; then
    echo "deny: not under --patched, which it cannot see: --published and CI run it" >> "$results/report"
else
    STEP_ALWAYS=1 step deny cargo deny check
fi

if [ -f scripts/gate_test.sh ]; then
    (   # What this script itself must do, on stubs, whenever a script has changed.
        STEP_PRINT=$(cat scripts/* | sha256sum | cut -c1-32) step selftest bash scripts/gate_test.sh
    ) &
fi
(   # Tests: one build, then the two runners side by side. nextest and `cargo test
    # --lib` are different gates (CLAUDE.md, rule 2) and share the build.
    if [ "$only_checks" = 1 ]; then
        exit 0
    fi
    if step build cargo nextest run --no-run --cargo-profile test-release "${config[@]}"; then
        step nextest cargo nextest run --cargo-profile test-release --no-fail-fast "${config[@]}" &
        step lib cargo test --profile test-release --lib "${config[@]}" &
        wait
        step doctest cargo test --profile test-release --doc "${config[@]}"
    fi
) &
(   # The debug profile's checks, which share its lock.
    step clippy cargo clippy --all-targets "${config[@]}" -- -D warnings
    if [ "$only_checks" = 0 ]; then
        cargo clean --doc "${config[@]}" > /dev/null 2>&1
        RUSTDOCFLAGS='-D warnings' step doc cargo doc --no-deps --document-private-items "${config[@]}"
    fi
) &
(   # The web's, which share theirs. The env replaces .cargo/config.toml's per-target
    # rustflags, exactly as it does on CI (#1321).
    RUSTFLAGS='-D warnings' step wasm cargo check --workspace --target wasm32-unknown-unknown "${config[@]}"
    RUSTFLAGS='-D warnings' step webgpu cargo check -p symbios-overlands --target wasm32-unknown-unknown --features webgpu "${config[@]}"
    if [ "$worker" = 1 ]; then
        # Always: a build is what puts this tree's worker where the size is read.
        if STEP_ALWAYS=1 step worker cargo build -p gen-worker --release --target wasm32-unknown-unknown "${config[@]}"; then
            if check_worker >> "$results/report" 2>&1; then
                touch "$results/ok-size"
            else
                touch "$results/failed"
            fi
        fi
    fi
) &
wait

# Only what still describes what was read counts.
bad=0
if [ "$(print_of)" != "$fingerprint" ]; then
    echo "gate: the tree, Cargo.lock or the tools changed during this run: its passes do not count" >> "$results/report"
    [ -e "$results/records" ] && xargs -r rm -f < "$results/records"
    bad=1
fi
[ -e "$results/failed" ] && bad=1
for name in "${owed[@]}"; do
    if [ ! -e "$results/ok-$name" ]; then
        grep -q "^$name: FAILED" "$results/report" || echo "$name: did not report" >> "$results/report"
        bad=1
    fi
done

cat "$results/report"
echo "gate: $(( $(date +%s) - started )) s"
if [ "$bad" = 1 ]; then
    echo "gate: FAILED"
    exit 1
fi
if [ "$patched" = 1 ]; then
    touch "$passed/published-$tested"
fi
echo "gate: passed"

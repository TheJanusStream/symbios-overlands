#!/usr/bin/env bash
# What the gate must never do, on a stub toolchain: say `passed` over a check that failed
# or did not run, skip one it owes, or leave a patched lockfile behind. Each case builds
# its own small world, side by side, and no real cargo runs.
#
#   scripts/gate_test.sh                   every case
#   scripts/gate_test.sh CASE...           just those
#
# GATE_UNDER_TEST names another copy of the scripts, to see a case fail without a fix.
set -u
scripts=${GATE_UNDER_TEST:-$(cd "$(dirname "$0")" && pwd)}

new_world() {
    w=$(mktemp -d) || exit 2
    status=0
    mkdir -p "$w/bin" "$w/home" "$w/repo/scripts" "$w/repo/src" "$w/repo/crates/gen-jobs" \
        "$w/repo/.github/workflows" "$w/sib"
    cp "$scripts/gate.sh" "$scripts/memguard.sh" "$w/repo/scripts/"
    printf 'target/\nCargo.lock\n' > "$w/repo/.gitignore"
    echo 'pub fn one() {}' > "$w/repo/src/lib.rs"
    printf '[dependencies]\nsymbios-avatar = "1.0"\nbevy_symbios_avatar = "1.0"\n' > "$w/repo/Cargo.toml"
    printf '[dependencies]\nsymbios-avatar = { version = "1.0", features = ["serde-avatar"] }\n' \
        > "$w/repo/crates/gen-jobs/Cargo.toml"
    echo 'check dist/gen-worker_bg.wasm 100000 100000' > "$w/repo/.github/workflows/deploy.yml"
    locked 1.0.0 > "$w/repo/Cargo.lock"
    git -C "$w/repo" init -q
    for sibling in symbios-avatar bevy_symbios_avatar; do
        mkdir -p "$w/sib/$sibling/src" "$w/sib/$sibling/docs"
        printf 'target/\nCargo.lock\n' > "$w/sib/$sibling/.gitignore"
        printf '[package]\nversion = "1.0.0"\nfeatures = "a"\nsymbios-avatar = "1.0"\n' > "$w/sib/$sibling/Cargo.toml"
        echo "pub fn $sibling() {}" | tr - _ > "$w/sib/$sibling/src/lib.rs"
        echo words > "$w/sib/$sibling/docs/a.md"
        git -C "$w/sib/$sibling" init -q
    done
    cat > "$w/bin/rustc" <<'STUB'
#!/bin/sh
echo rustc stub
STUB
    cat > "$w/bin/wasm-bindgen" <<'STUB'
#!/bin/sh
mkdir -p "$2"
echo "${STUB_WASM:-small}" > "$2/gen-worker_bg.wasm"
STUB
    cat > "$w/bin/cargo" <<'STUB'
#!/bin/sh
args="$*"
echo "$args" >> "$STUB/calls"
if [ -n "${STUB_RULE:-}" ]; then eval "$STUB_RULE"; fi
case "$args" in
    -V) echo cargo stub ;;
    'nextest --version') echo nextest stub ;;
    update*)
        awk -v RS= -v ORS='\n\n' '/name = "(symbios-avatar|bevy_symbios_avatar)"/ { gsub(/\nsource = [^\n]*/, "") } { print }' \
            Cargo.lock > Cargo.lock.new && mv Cargo.lock.new Cargo.lock ;;
    tree*)
        if awk -v RS= -v name="${args#tree -p }" 'index($0, "\"" substr(name, 1, index(name, " ") - 1) "\"") && /source/ { found = 1 } END { exit !found }' Cargo.lock
        then echo 'crate v1.0.0'
        else echo 'crate v1.0.0 (/somewhere)'; fi ;;
    *'-p gen-worker'*)
        mkdir -p target/wasm32-unknown-unknown/release
        echo wasm > target/wasm32-unknown-unknown/release/gen-worker.wasm ;;
esac
exit 0
STUB
    chmod +x "$w/bin/rustc" "$w/bin/cargo" "$w/bin/wasm-bindgen"
    trap 'rm -rf "$w"' EXIT
}

# A lockfile on the registry, at a version of the avatar crates.
locked() {
    local name
    for name in bevy_symbios_avatar other symbios-avatar; do
        printf '[[package]]\nname = "%s"\nversion = "%s"\nsource = "registry"\n\n' "$name" "$1"
    done
}

# What crates.io would hold once a sibling here is published, as it is now.
publish() {
    local sibling found
    for sibling in symbios-avatar bevy_symbios_avatar; do
        found=$w/home/.cargo/registry/src/x/$sibling-$1
        rm -rf "$found"
        mkdir -p "$found"
        cp -r "$w/sib/$sibling/src" "$w/sib/$sibling/docs" "$w/sib/$sibling/.gitignore" "$found/"
        cp "$w/sib/$sibling/Cargo.toml" "$found/Cargo.toml.orig"
        echo normalised > "$found/Cargo.toml"
        echo generated > "$found/.cargo_vcs_info.json"
    done
}

# gate ARGS...: one run in this world, its output kept and its code in $code.
gate() {
    (cd "$w/repo" && exec env HOME="$w/home" PATH="$w/bin:$PATH" STUB="$w" GATE_SIBLINGS="$w/sib" \
        GATE_JOBS=2 GATE_MEM_FLOOR=0 scripts/gate.sh "$@") > "$w/out" 2>&1
    code=$?
}
wrong() { echo "  $*"; status=1; }
code_is() { [ "$code" = "$1" ] || wrong "exit $code, not $1: $(tail -3 "$w/out" | tr '\n' ';')"; }
said() { grep -qe "$1" "$w/out" || wrong "did not say: $1"; }
not_said() { grep -qe "$1" "$w/out" && wrong "said: $1"; }
called() { grep -qe "$1" "$w/calls" || wrong "never called: $1"; }
not_called() { grep -qe "$1" "$w/calls" && wrong "called: $1"; }
count() { grep -ce "$1" "$w/calls"; }

suite='^nextest run --cargo-profile'

case_runs_every_step() {
    gate
    code_is 0; said '^gate: passed'
    for line in '^fmt --all -- --check$' '^deny check$' '^clippy --all-targets -- -D warnings$' \
        '^nextest run --no-run --cargo-profile test-release$' "$suite" \
        '^test --profile test-release --lib$' '^test --profile test-release --doc$' \
        '^doc --no-deps --document-private-items$' '^check --workspace --target wasm32-unknown-unknown$' \
        '^check -p symbios-overlands --target wasm32-unknown-unknown --features webgpu$'; do
        called "$line"
    done
}
case_a_repeat_runs_nothing_but_deny() {
    gate; code_is 0
    gate; code_is 0; said 'nextest: passed already'
    [ "$(count "$suite")" = 1 ] || wrong "the suite ran again"
    [ "$(count '^deny check$')" = 2 ] || wrong "deny was taken from its record"
}
case_a_failed_step_fails() {
    STUB_RULE='case "$args" in *webgpu*) exit 1 ;; esac' gate
    code_is 1; said '^gate: FAILED'; not_said '^gate: passed'
    STUB_RULE='' gate
    code_is 0
    [ "$(count 'webgpu')" = 2 ] || wrong "the failed step left a record"
}
case_a_failed_build_fails_and_runs_no_tests() {
    STUB_RULE='case "$args" in *--no-run*) exit 1 ;; esac' gate
    code_is 1; not_said '^gate: passed'
    not_called "$suite"
}
case_a_dead_lane_fails() {
    STUB_RULE='[ "$args" = "clippy --all-targets -- -D warnings" ] && kill -9 "$(awk "{print \$4}" /proc/$PPID/stat)"' gate
    code_is 1; said 'doc: did not report'; not_said '^gate: passed'
}
case_an_edit_during_a_run_fails_and_keeps_no_record() {
    cp "$w/repo/src/lib.rs" "$w/lib.rs"
    STUB_RULE='case "$args" in clippy*) echo "// edited" >> src/lib.rs ;; esac' gate
    code_is 1; said 'changed during this run'; not_said '^gate: passed'
    cp "$w/lib.rs" "$w/repo/src/lib.rs"
    STUB_RULE='' gate
    code_is 0
    [ "$(count "$suite")" = 2 ] || wrong "a pass from the edited run was kept"
}
case_a_step_that_moves_the_lockfile_fails() {
    STUB_RULE='case "$args" in clippy*) echo "# moved" >> Cargo.lock ;; esac' gate
    code_is 1; said 'changed during this run'; not_said '^gate: passed'
}
case_an_edit_between_runs_runs_again() {
    gate; code_is 0
    echo '// edited' >> "$w/repo/src/lib.rs"
    gate; code_is 0
    [ "$(count "$suite")" = 2 ] || wrong "the edit went unseen"
}
case_a_step_print_from_outside_keys_nothing() {
    STEP_PRINT=leaked gate; code_is 0
    echo '// edited' >> "$w/repo/src/lib.rs"
    STEP_PRINT=leaked gate; code_is 0
    [ "$(count "$suite")" = 2 ] || wrong "a record was keyed on the caller's variable"
}
case_staging_a_file_moves_nothing() {
    echo new > "$w/repo/src/new.txt"
    gate; code_is 0
    git -C "$w/repo" add src/new.txt
    gate; code_is 0
    [ "$(count "$suite")" = 1 ] || wrong "staging ran the suite again"
}
case_a_file_git_quotes_is_still_read() {
    echo one > "$w/repo/src/café.txt"
    gate; code_is 0
    echo two > "$w/repo/src/café.txt"
    gate; code_is 0
    [ "$(count "$suite")" = 2 ] || wrong "a change to a non-ASCII name went unseen"
}
case_the_worker_over_its_budget_fails() {
    echo 'check dist/gen-worker_bg.wasm 2 100000' > "$w/repo/.github/workflows/deploy.yml"
    gate --worker
    code_is 1; said '^worker: raw'; not_said '^gate: passed'
}
case_the_worker_is_built_every_run() {
    gate --worker; code_is 0
    gate --worker; code_is 0
    [ "$(count 'gen-worker')" = 2 ] || wrong "the size was read off a build from another run"
}
case_patched_keeps_deny_out_and_puts_the_lock_back() {
    cp -p "$w/repo/Cargo.lock" "$w/lock"
    gate --patched
    code_is 0; said 'deny: not under --patched'
    not_called '^deny'
    called '^nextest run --cargo-profile test-release --no-fail-fast --config'
    called '^clean --doc --config'
    cmp -s "$w/lock" "$w/repo/Cargo.lock" || wrong "the lock was not put back"
    [ "$w/repo/Cargo.lock" -nt "$w/lock" ] && wrong "the lock has a new time"
}
case_patched_refuses_a_dead_patch() {
    cp -p "$w/repo/Cargo.lock" "$w/lock"
    STUB_RULE='case "$args" in update*) exit 0 ;; esac' gate --patched
    code_is 2; said 'not in the build'; not_said '^gate: passed'
    cmp -s "$w/lock" "$w/repo/Cargo.lock" || wrong "the lock was not put back"
}
case_patched_refuses_a_copy_git_does_not_hold() {
    rm -rf "$w/sib/symbios-avatar/.git"
    gate --patched
    code_is 2; said 'not a git checkout'; not_said '^gate: passed'
}
case_a_stopped_run_stops_its_steps_and_puts_the_lock_back() {
    cp -p "$w/repo/Cargo.lock" "$w/lock"
    (cd "$w/repo" && exec env HOME="$w/home" PATH="$w/bin:$PATH" STUB="$w" GATE_SIBLINGS="$w/sib" \
        GATE_JOBS=2 GATE_MEM_FLOOR=0 \
        STUB_RULE='case "$args" in *--no-run*) echo $$ > "$STUB/sleeper"; exec sleep 60 ;; esac' \
        scripts/gate.sh --patched) > "$w/out" 2>&1 &
    local run=$! tries=0
    until [ -s "$w/sleeper" ] || [ "$tries" = 200 ]; do sleep 0.1; tries=$((tries + 1)); done
    kill -TERM "$run"
    wait "$run"
    code=$?
    code_is 130
    kill -0 "$(cat "$w/sleeper")" 2>/dev/null && wrong "a step outlived the run"
    cmp -s "$w/lock" "$w/repo/Cargo.lock" || wrong "the lock was not put back"
    gate
    code_is 0
}
case_a_killed_runs_lock_goes_back_on_the_next_run() {
    locked 1.0.0 > "$w/original"
    mkdir -p "$w/repo/target/gate"
    cp "$w/original" "$w/repo/target/gate/Cargo.lock.kept"
    echo patched > "$w/repo/Cargo.lock"
    gate
    code_is 0
    cmp -s "$w/original" "$w/repo/Cargo.lock" || wrong "the kept lock was not put back"
}
case_a_second_run_is_refused_and_leaves_the_lock() {
    mkdir -p "$w/repo/target/gate"
    cp "$w/repo/Cargo.lock" "$w/lock"
    echo running > "$w/repo/target/gate/Cargo.lock.kept"
    flock "$w/repo/target/gate/lock" sleep 20 &
    local holder=$!
    sleep 0.3
    gate
    code_is 2; said 'still going'; not_said '^gate: passed'
    cmp -s "$w/lock" "$w/repo/Cargo.lock" || wrong "a refused run touched the lock"
    kill "$holder" 2>/dev/null
    pkill -P "$holder" 2>/dev/null
}
case_published_on_the_source_it_tested_skips_the_tests() {
    gate --patched; code_is 0
    publish 1.0.0
    : > "$w/calls"
    gate --published
    code_is 0; said 'no tests and no doc gate'
    not_called "$suite"; not_called '^doc'
    called '^deny check$'; called '^clippy'; called 'webgpu'; called 'wasm32-unknown-unknown$'
}
case_published_after_a_real_bump_skips_the_tests() {
    gate --patched; code_is 0
    sed -i 's/^version = "1.0.0"/version = "1.1.0"/; s/^symbios-avatar = "1.0"/symbios-avatar = "1.1"/' \
        "$w/sib/symbios-avatar/Cargo.toml" "$w/sib/bevy_symbios_avatar/Cargo.toml"
    publish 1.1.0
    sed -i -E 's/^((symbios-avatar|bevy_symbios_avatar) = (\{ version = )?)"1.0"/\1"1.1"/' \
        "$w/repo/Cargo.toml" "$w/repo/crates/gen-jobs/Cargo.toml"
    locked 1.1.0 | sed 's/name = "other"/name = "other"/' > "$w/repo/Cargo.lock.new"
    awk -v RS= -v ORS='\n\n' '/name = "other"/ { sub(/version = "1.1.0"/, "version = \"1.0.0\"") } { print }' \
        "$w/repo/Cargo.lock.new" > "$w/repo/Cargo.lock"
    rm "$w/repo/Cargo.lock.new"
    : > "$w/calls"
    gate --published
    code_is 0; said 'no tests and no doc gate'
    not_called "$suite"
}
case_published_with_another_crate_moved_runs_the_tests() {
    gate --patched; code_is 0
    publish 1.0.0
    awk -v RS= -v ORS='\n\n' '/name = "other"/ { sub(/version = "1.0.0"/, "version = \"1.0.1\"") } { print }' \
        "$w/repo/Cargo.lock" > "$w/repo/Cargo.lock.new"
    mv "$w/repo/Cargo.lock.new" "$w/repo/Cargo.lock"
    : > "$w/calls"
    gate --published
    code_is 0; said 'no passed --patched run'
    called "$suite"
}
case_published_on_a_doc_byte_runs_the_tests() {
    gate --patched; code_is 0
    publish 1.0.0
    echo more >> "$w/home/.cargo/registry/src/x/symbios-avatar-1.0.0/docs/a.md"
    : > "$w/calls"
    gate --published
    code_is 0; said 'is not the source here'
    called "$suite"; called '^doc'
}
case_published_on_a_manifest_edit_runs_the_tests() {
    gate --patched; code_is 0
    sed -i 's/^features = "a"/features = "b"/' "$w/sib/symbios-avatar/Cargo.toml"
    publish 1.0.0
    : > "$w/calls"
    gate --published
    code_is 0; said 'no passed --patched run'
    called "$suite"
}
case_published_after_a_failed_patched_run_runs_the_tests() {
    STUB_RULE='case "$args" in clippy*) exit 1 ;; esac' gate --patched; code_is 1
    publish 1.0.0
    : > "$w/calls"
    STUB_RULE='' gate --published
    code_is 0; said 'no passed --patched run'
    called "$suite"
}
case_published_with_force_runs_the_tests() {
    gate --patched; code_is 0
    publish 1.0.0
    : > "$w/calls"
    gate --published --force
    code_is 0
    called "$suite"
}
case_published_with_a_copy_git_does_not_hold_runs_the_tests() {
    gate --patched; code_is 0
    publish 1.0.0
    rm -rf "$w/sib/symbios-avatar/.git"
    : > "$w/calls"
    gate --published
    code_is 0; said 'not a git checkout'
    called "$suite"
}

out=$(mktemp -d) || exit 2
trap 'rm -rf "$out"' EXIT
if [ "$#" -gt 0 ]; then
    cases=("$@")
else
    mapfile -t cases < <(declare -F | awk '{print $3}' | grep '^case_')
fi
pids=()
for this_case in "${cases[@]}"; do
    ( declare -F "$this_case" > /dev/null || exit 3; new_world; "$this_case"; exit "$status" ) \
        > "$out/$this_case" 2>&1 &
    pids+=("$!")
done
failed=0
for i in "${!cases[@]}"; do
    if ! wait "${pids[$i]}"; then
        echo "FAILED ${cases[$i]}"
        sed 's/^/ /' "$out/${cases[$i]}"
        failed=$((failed + 1))
    fi
done
echo "gate_test: ${#cases[@]} cases, $failed failed"
[ "$failed" = 0 ]

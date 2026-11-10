# SPDX-License-Identifier: MIT
# Shared by scripts/verify-*.sh: run as the repo's owner, report each result as
# it is found, summarise at the end. Source it after `cd` to the repo root.

# Re-runs the calling script as the repo's owner when started by someone else
# (the sandbox's desktop terminal is root; PostgreSQL refuses to run as root,
# and root-owned files would break the owner's builds).
verify_as_owner() {
  local self="$1"
  shift
  local owner
  owner="$(stat -c %U "$PWD")"
  [ "$(id -un)" = "$owner" ] && return 0
  if [ "$(id -u)" = 0 ] && command -v runuser > /dev/null; then
    echo "== re-running as $owner (the repo's owner) =="
    exec runuser -u "$owner" -- env HOME="$(getent passwd "$owner" | cut -d: -f6)" \
      PATH="$PATH" LANG=C.UTF-8 bash "$self" "$@"
  fi
  echo "FAIL run as $owner, who owns $PWD"
  exit 1
}

VERIFY_PASSED=0
VERIFY_FAILED=0
ok() { echo "ok   $*"; VERIFY_PASSED=$((VERIFY_PASSED + 1)); }
bad() { echo "FAIL $*"; VERIFY_FAILED=$((VERIFY_FAILED + 1)); }
# check DESCRIPTION COMMAND...: ok if the command succeeds (output hidden)
check() {
  local description="$1"
  shift
  if "$@" > /dev/null 2>&1; then ok "$description"; else bad "$description"; fi
}
# expect DESCRIPTION EXPECTED ACTUAL
expect() {
  if [ "$2" != "$3" ]; then
    bad "$1 (expected '$2', got '$3')"
  elif [ -n "$3" ]; then
    ok "$1: $3"
  else
    ok "$1"
  fi
}
free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
port_open() { (exec 3<> "/dev/tcp/127.0.0.1/$1") 2> /dev/null; }
http_code() { curl -s -m 5 -o /dev/null -w '%{http_code}' "$@" || true; }
# wait_http URL [SECONDS]: until the URL answers at all
wait_http() {
  local deadline=$((SECONDS + ${2:-30}))
  while [ "$SECONDS" -lt "$deadline" ]; do
    [ "$(http_code "$1")" != 000 ] && return 0
    sleep 0.2
  done
  return 1
}
verify_summary() {
  echo
  local total=$((VERIFY_PASSED + VERIFY_FAILED))
  if [ "$VERIFY_FAILED" = 0 ]; then
    echo "PASS: $1 verified ($VERIFY_PASSED checks)"
  else
    echo "FAIL: $VERIFY_FAILED of $total checks failed"
    exit 1
  fi
}

# pg_start: a throwaway PostgreSQL 16 from the local binaries, set up like
# gluon-local's user-postgres (user/user/user, password over TCP). Sets
# PG_ENV (POSTGRES_* assignments for `env`), PG_DIR and PG_BIN; pg_stop removes it.
pg_start() {
  PG_BIN="${CONTRACT_PG_BIN:-$(dirname "$(realpath "$(command -v pg_ctl)")")}"
  PG_DIR="$(mktemp -d /tmp/verify-pg.XXXXXX)"
  local port
  port="$(free_port)"
  echo user > "$PG_DIR/password"
  "$PG_BIN/initdb" -D "$PG_DIR/db" -U user --pwfile="$PG_DIR/password" --auth-local=trust \
    --auth-host=scram-sha-256 --locale=C.UTF-8 > "$PG_DIR/initdb.log" 2>&1 \
    && "$PG_BIN/pg_ctl" start -D "$PG_DIR/db" -l "$PG_DIR/log" -w \
      -o "-c listen_addresses=127.0.0.1 -c port=$port -c unix_socket_directories=$PG_DIR" > /dev/null \
    && "$PG_BIN/createdb" -h "$PG_DIR" -p "$port" -U user user
  PG_ENV="POSTGRES_HOST=127.0.0.1 POSTGRES_PORT=$port POSTGRES_DB=user POSTGRES_USER=user POSTGRES_PASSWORD=user"
}
pg_stop() {
  [ -n "${PG_DIR:-}" ] || return 0
  "$PG_BIN/pg_ctl" stop -D "$PG_DIR/db" -m fast > /dev/null 2>&1
  rm -rf "$PG_DIR"
  PG_DIR=""
}

# contract_checks PYTEST_ARGS...: runs checks from gluon's tests/contract (paths
# relative to it) against this repo's build, reporting each one as it is found.
contract_checks() {
  local repo="$PWD" contract log count=0
  contract="$(realpath "$repo/../../tests/contract")"
  log="$(mktemp)"
  if ! cargo build -q --locked -p user-service > "$log" 2>&1; then
    bad "cargo build failed"
    tail -5 "$log" | sed 's/^/     /'
    rm -f "$log"
    return
  fi
  (cd "$contract" && uv run --no-sync pytest -p no:cacheprovider -rA --tb=line -q "$@" \
    --target-cmd "$repo/target/debug/user-service" --target-cwd "$repo" --target-name user-service-rs) > "$log" 2>&1
  while read -r outcome test rest; do
    count=$((count + 1))
    if [ "$outcome" = PASSED ]; then ok "contract: ${test##*::}"; else bad "contract: ${test##*::} $rest"; fi
  done < <(grep -E '^(PASSED|FAILED|ERROR) [^ ]+::' "$log")
  if [ "$count" = 0 ]; then
    bad "no contract checks ran:"
    tail -5 "$log" | sed 's/^/     /'
  fi
  rm -f "$log"
}

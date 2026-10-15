#!/usr/bin/env bash
# capture22w.sh SUBCOMMAND
#
# Phase 22 (22-12) capture driver for the Windows OpenSSH capture host
# (WFLWVM01 in the plan; an Azure Windows Server 2022 VM by user decision of
# 2026-10-14, see the dated amendment in 22-CONTEXT.md). Everything here runs
# as the NON-ADMIN service account over ssh and sftp, the way the Phase 22
# transport will:
#
#   preflight  keyscan vs the pinned key, `whoami`, ign status, deploymentMode
#   c7a        Windows OpenSSH behaviours (D-15): sftp path forms, listing
#              format, error signalling, rename over an existing target,
#              junctions, the default shell and PowerShell -EncodedCommand
#              exit codes and CLIXML, command length, paths over 260 chars,
#              host key types, the account's groups
#   c6         what the service account can write and read (Decision B)
#   c5ntfs     on-disk names the gateway writes on NTFS for hostile tag names
#   clean      remove every scratch artefact this script may have left
#
# Environment (either set of names):
#   IGN_WFLWVM01_HOST | IGN_WINVM_HOST          host name or address
#   IGN_WFLWVM01_KEY  | IGN_WINVM_KEY           private key TEXT (ed25519)
#   IGN_WFLWVM01_PROFILE | IGN_WINVM_PROFILE    ign profile for the gateway
#   IGN_WFLWVM01_USER (default svc-ignition-deploy), IGN_WFLWVM01_PORT (22),
#   IGN_WFLWVM01_KNOWN_HOSTS (default $HOME/.cache/ign-wflwvm01/known_hosts)
#
# The ssh and sftp argv carry the exact `-o` set and order of
# `hardened_options` in crates/ignition-core/src/deploy/transport/remote.rs
# (StrictHostKeyChecking=yes against the pinned file, `--` before the
# destination). The pinned known_hosts file MUST exist before anything touches
# the network: the script exits 2 without a single connection when it is
# missing. The key text is written to a 0700 temp dir (CRLF and trailing
# newline normalised like `normalize_private_key`) and removed by an EXIT
# trap; it is never printed, and every command line is stamped with `<key>`.
# Never enable `set -x` here.
#
# Every block appended to 22-LIVE-CAPTURES.md carries the exact command and a
# UTC time taken just before the run. Sections are spliced in ahead of
# `## Decisions`. A section that already exists is refused (exit 2) unless
# CAPTURE_REPLACE=1.
set -euo pipefail
# Byte-wise text tools: cmd.exe writes OEM-codepage bytes that are not UTF-8.
export LC_ALL=C

HOST=${IGN_WFLWVM01_HOST:-${IGN_WINVM_HOST:-}}
KEYTEXT=${IGN_WFLWVM01_KEY:-${IGN_WINVM_KEY:-}}
PROFILE=${IGN_WFLWVM01_PROFILE:-${IGN_WINVM_PROFILE:-}}
SSH_USER=${IGN_WFLWVM01_USER:-svc-ignition-deploy}
PORT=${IGN_WFLWVM01_PORT:-22}
PIN=${IGN_WFLWVM01_KNOWN_HOSTS:-$HOME/.cache/ign-wflwvm01/known_hosts}

# --- gate 1: the pinned known_hosts file, before any network call ----------
if [ ! -s "$PIN" ]; then
  echo "REFUSING: pinned known_hosts file '$PIN' is missing or empty." >&2
  echo "Pin the host key only after its ssh-keyscan fingerprint equals the one read on the host console (T-22-50)." >&2
  exit 2
fi
case "$(stat -f '%Lp' "$PIN" 2>/dev/null || stat -c '%a' "$PIN")" in
  600 | 400) ;;
  *) echo "REFUSING: pinned known_hosts '$PIN' must have mode 0600" >&2; exit 2 ;;
esac

SUB=${1:-}
case "$SUB" in
  preflight | c7a | c6 | c5ntfs | clean) ;;
  *) echo "usage: capture22w.sh preflight|c7a|c6|c5ntfs|clean" >&2; exit 64 ;;
esac

[ -n "$HOST" ] || { echo "IGN_WFLWVM01_HOST (or IGN_WINVM_HOST) is not set" >&2; exit 2; }
[ -n "$KEYTEXT" ] || { echo "IGN_WFLWVM01_KEY (or IGN_WINVM_KEY) is not set" >&2; exit 2; }
[ -n "$PROFILE" ] || { echo "IGN_WFLWVM01_PROFILE (or IGN_WINVM_PROFILE) is not set" >&2; exit 2; }
case "$HOST$SSH_USER" in
  -* | *[[:space:]]* | *[[:cntrl:]]*) echo "REFUSING: host or user is malformed" >&2; exit 2 ;;
esac

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md}
IGN=${IGN_BIN:-ign}
DEST="$SSH_USER@$HOST"

TMPD=$(mktemp -d)
chmod 700 "$TMPD"
SEC=$TMPD/section.md
: > "$SEC"
LOC=$TMPD/local
mkdir -p "$LOC"

# --- the key: 0600 file in the 0700 temp dir, normalised, removed on exit ---
KEYFILE=$TMPD/key
(
  umask 077
  # normalize_private_key: CRLF and lone CR to LF, exactly one trailing LF.
  printf '%s' "$KEYTEXT" | perl -0pe 's/\r\n/\n/g; s/\r/\n/g; s/\n+\z//; $_ .= "\n"' > "$KEYFILE"
  chmod 600 "$KEYFILE"
)
# One body line of the key, used to refuse any output that contains it.
KEYBODY=$(sed -n '2p' "$KEYFILE")
unset KEYTEXT
case "$(head -n1 "$KEYFILE")" in
  "-----BEGIN OPENSSH PRIVATE"*) ;;
  *) echo "the key text is not an OpenSSH private key" >&2; exit 2 ;;
esac

SPLICED=0
splice_section() {
  [ "$SPLICED" = 1 ] && return 0
  SPLICED=1
  [ -s "$SEC" ] || return 0
  awk -v secfile="$SEC" '
    /^## Decisions$/ && !done { while ((getline line < secfile) > 0) print line; print ""; done=1 }
    { print }
    END { if (!done) { while ((getline line < secfile) > 0) print line } }
  ' "$OUT" > "$OUT.new" && mv "$OUT.new" "$OUT"
}
on_exit() {
  local rc=$?
  if [ $rc -ne 0 ] && [ -s "$SEC" ]; then
    printf '\n[capture aborted with exit %s at %s; the blocks above are what ran]\n' "$rc" "$(date -u +%FT%TZ)" >> "$SEC"
  fi
  splice_section || true
  /bin/rm -rf "$TMPD"
}
trap on_exit EXIT

# --- the hardened option set, in the fixed order of hardened_options -------
optval_path() {
  local v=$1
  v=${v//%/%%}
  case "$v" in *[[:space:]]*) v="\"$v\"" ;; esac
  printf '%s' "$v"
}
OPTS=(
  -o BatchMode=yes
  -o StrictHostKeyChecking=yes
  -o "UserKnownHostsFile=$(optval_path "$PIN")"
  -o GlobalKnownHostsFile=/dev/null
  -o UpdateHostKeys=no
  -o VerifyHostKeyDNS=no
  -o PreferredAuthentications=publickey
  -o PasswordAuthentication=no
  -o KbdInteractiveAuthentication=no
  -o NumberOfPasswordPrompts=0
  -o IdentitiesOnly=yes
  -o ForwardAgent=no
  -o ForwardX11=no
  -o ClearAllForwardings=yes
  -o PermitLocalCommand=no
  -o ControlMaster=no
  -o ControlPath=none
  -o ConnectTimeout=10
  -o ServerAliveInterval=5
  -o ServerAliveCountMax=3
  -o LogLevel=ERROR
)
# RemoteShell::ssh_argv: -T -p <port> -i <key> -o k=v ... -- user@host [command]
SSH_ARGS=(-T -p "$PORT" -i "$KEYFILE" "${OPTS[@]}")
# RemoteShell::sftp_argv: -q -b - -P <port> -i <key> -o k=v ... -- user@host
SFTP_ARGS=(-q -b - -P "$PORT" -i "$KEYFILE" "${OPTS[@]}")

DATA_WIN='C:\Program Files\Inductive Automation\Ignition\data'
DATA_FWD='C:/Program Files/Inductive Automation/Ignition/data'
DATA_SFTP="/$DATA_FWD"
RES_SFTP="$DATA_SFTP/config/resources"
RES_WIN="$DATA_WIN\\config\\resources"

# --- stamping and block helpers -------------------------------------------
utc() { date -u +%FT%TZ; }
fix_key_path() { sed -e "s|$KEYFILE|<key>|g" -e "s|$TMPD|\$TMP|g" -e "s|$HOME|\$HOME|g"; }
OPTS_SHOWN=""
for _o in "${OPTS[@]}"; do OPTS_SHOWN+=$(printf '%q ' "$_o"); done
# show_argv ARG...: the argv as a command line. The key path is <key>; the
# 21-option hardened set (printed in full once per section as H) is <H>; any
# argument over 360 chars is cut with its length stated.
show_argv() {
  local a out=""
  for a in "$@"; do
    if [ "${#a}" -gt 360 ]; then a="${a:0:160}...[${#a} chars]"; fi
    out+=$(printf '%q ' "$a")
  done
  out=${out//"$OPTS_SHOWN"/<H> }
  printf '%s' "$out" | fix_key_path
}
h_def() { prose "\`<H>\` stands for exactly these ssh/sftp options, in this order (the \`hardened_options\` set with an identity): \`$(printf '%s' "$OPTS_SHOWN" | fix_key_path)\`"; }

sect() { printf '\n## %s\n' "$1" >> "$SEC"; }
sub() { printf '\n### %s\n' "$1" >> "$SEC"; }
prose() { printf '\n%s\n' "$1" >> "$SEC"; }

refuse_if_present() {
  local id=$1
  if grep -q "^## $id" "$OUT"; then
    if [ "${CAPTURE_REPLACE:-0}" = 1 ]; then
      awk -v id="## $id" '
        index($0, id) == 1 { skip = 1; next }
        /^## / && skip { skip = 0 }
        !skip { print }
      ' "$OUT" > "$OUT.new" && mv "$OUT.new" "$OUT"
    else
      echo "REFUSING: $OUT already holds a '## $id' section (CAPTURE_REPLACE=1 replaces it)" >&2
      exit 2
    fi
  fi
}

LAST_OUT=""
LAST_ERR=""
LAST_RC=0
NOTE=""
# run_block LABEL STDIN_TEXT ARGV...: run ARGV with STDIN_TEXT on stdin
# (empty: /dev/null), one verbatim block appended. A 150 s alarm bounds every
# command. Refuses (exit 2, nothing appended) when an output holds a line of
# the private key.
run_block() {
  local label=$1 stdin_text=$2; shift 2
  local so=$TMPD/so sr=$TMPD/se si=$TMPD/si started rc=0
  if [ -n "$stdin_text" ]; then printf '%s\n' "$stdin_text" > "$si"; else : > "$si"; fi
  started=$(utc)
  perl -e 'alarm 150; exec @ARGV' "$@" < "$si" > "$so" 2> "$sr" || rc=$?
  if [ -n "$KEYBODY" ] && { grep -qF -- "$KEYBODY" "$so" || grep -qF -- "$KEYBODY" "$sr"; }; then
    echo "REFUSING: command output contains private key text; nothing appended" >&2
    exit 2
  fi
  local cr_o cr_e
  cr_o=$(tr -cd '\r' < "$so" | wc -c | tr -d ' ')
  cr_e=$(tr -cd '\r' < "$sr" | wc -c | tr -d ' ')
  tr -d '\r' < "$so" > "$so.n"
  tr -d '\r' < "$sr" > "$sr.n"
  local esc_o=0 esc_e=0
  if ! iconv -f UTF-8 -t UTF-8 "$so.n" > /dev/null 2>&1; then
    esc_o=1; perl -pe 's/([^\x09\x0a\x20-\x7e])/sprintf("\\x%02X", ord($1))/ge' "$so.n" > "$so.x"; mv "$so.x" "$so.n"
  fi
  if ! iconv -f UTF-8 -t UTF-8 "$sr.n" > /dev/null 2>&1; then
    esc_e=1; perl -pe 's/([^\x09\x0a\x20-\x7e])/sprintf("\\x%02X", ord($1))/ge' "$sr.n" > "$sr.x"; mv "$sr.x" "$sr.n"
  fi
  LAST_OUT=$(cat "$so.n")
  LAST_ERR=$(cat "$sr.n")
  # A runaway listing is cut: first 100 and last 30 lines, the cut stated.
  local lines_o
  lines_o=$(wc -l < "$so.n" | tr -d ' ')
  if [ "$lines_o" -gt 160 ]; then
    { head -n 100 "$so.n"; printf '[... %s stdout lines omitted by the driver ...]\n' "$((lines_o - 130))"; tail -n 30 "$so.n"; } > "$so.c"
    mv "$so.c" "$so.n"
  fi
  LAST_RC=$rc
  {
    printf '\n#### %s\n\n`%s` @ %s\n\n~~~\n' "$label" "$(show_argv "$@")" "$started"
    if [ -n "$stdin_text" ]; then
      printf '[stdin]\n%s\n' "$stdin_text" | fix_key_path
    fi
    if [ -n "$NOTE" ]; then printf '[script]\n%s\n' "$NOTE"; fi
    printf '[stdout]\n'
    cat "$so.n"
    if [ -s "$so.n" ] && [ "$(tail -c1 "$so.n" | wc -l | tr -d ' ')" = 0 ]; then printf '\n[stdout did not end with a newline]\n'; fi
    [ "$cr_o" = 0 ] || printf '[%s CR bytes removed from stdout]\n' "$cr_o"
    [ "$esc_o" = 0 ] || printf '[stdout is not valid UTF-8: every byte outside printable ASCII is shown as \\xNN]\n'
    printf '[stderr]\n'
    cat "$sr.n"
    if [ -s "$sr.n" ] && [ "$(tail -c1 "$sr.n" | wc -l | tr -d ' ')" = 0 ]; then printf '\n[stderr did not end with a newline]\n'; fi
    [ "$cr_e" = 0 ] || printf '[%s CR bytes removed from stderr]\n' "$cr_e"
    [ "$esc_e" = 0 ] || printf '[stderr is not valid UTF-8: every byte outside printable ASCII is shown as \\xNN]\n'
    printf '[exit %s]\n~~~\n' "$rc"
  } >> "$SEC"
  NOTE=""
}

# rcap LABEL REMOTE_COMMAND: ssh -T ... -- user@host REMOTE_COMMAND.
rcap() { run_block "$1" "" ssh "${SSH_ARGS[@]}" -- "$DEST" "$2"; }
# scap LABEL LINE...: sftp -q -b - ... with the lines as the batch on stdin.
scap() {
  local label=$1; shift
  local batch
  batch=$(printf '%s\n' "$@")
  run_block "$label" "$batch" sftp "${SFTP_ARGS[@]}" -- "$DEST"
}
# quiet variants (no block) used to read a fact a later block depends on.
rq() { perl -e 'alarm 150; exec @ARGV' ssh "${SSH_ARGS[@]}" -- "$DEST" "$1" < /dev/null 2>/dev/null | tr -d '\r'; }
sq() {
  printf '%s\n' "$@" | perl -e 'alarm 150; exec @ARGV' sftp "${SFTP_ARGS[@]}" -- "$DEST" 2>/dev/null | tr -d '\r'
}

# PowerShell: RemoteCommand::powershell_encoded (UTF-16LE base64).
PS_PRELUDE="\$ProgressPreference='SilentlyContinue'; \$ErrorActionPreference='Stop'; "
ps_command() {
  local b64
  b64=$(printf '%s' "$1" | iconv -f UTF-8 -t UTF-16LE | base64 | tr -d '\n')
  printf 'powershell -NoProfile -NonInteractive -EncodedCommand %s' "$b64"
}
# pscap LABEL SCRIPT [prelude]: the script text is recorded under [script].
pscap() {
  local label=$1 script=$2 mode=${3:-bare}
  if [ "$mode" = prelude ]; then script="$PS_PRELUDE$script"; fi
  NOTE=$script
  run_block "$label" "" ssh "${SSH_ARGS[@]}" -- "$DEST" "$(ps_command "$script")"
}

ign_json() { "$IGN" --profile "$PROFILE" --json "$@"; }
# icap LABEL ARG...: an `ign` invocation as a block (no secrets: the token
# lives in the profile's secret chain, never in the command).
icap() { local label=$1; shift; run_block "$label" "" "$IGN" --profile "$PROFILE" --json "$@"; }

win_home() { rq 'echo %USERPROFILE%' | head -n1 | tr -d '\r\n'; }

scratch_clean() {
  local home
  home=$(win_home)
  [ -n "$home" ] || return 0
  local script
  script="\$ProgressPreference='SilentlyContinue'; \$ErrorActionPreference='Continue'; \$p=Join-Path \$env:USERPROFILE 'ign-c7a'; if(Test-Path -LiteralPath \$p){ foreach(\$n in 'jx\\junc','jx\\loop','jx\\symd'){ \$q=Join-Path \$p \$n; if(Test-Path -LiteralPath \$q){ cmd /c rmdir \"\$q\" } }; Remove-Item -LiteralPath ('\\\\?\\'+\$p) -Recurse -Force; if(Test-Path -LiteralPath \$p){ cmd /c rd /s /q ('\\\\?\\'+\$p) } }; if(Test-Path -LiteralPath \$p){ 'STILL PRESENT' } else { 'ign-c7a removed' }"
  NOTE=$script
  run_block "cleanup: remove the account-home scratch directory ign-c7a" "" ssh "${SSH_ARGS[@]}" -- "$DEST" "$(ps_command "$script")"
}

# ===========================================================================
preflight() {
  echo "== keyscan vs pin"
  local scan=$TMPD/scan
  ssh-keyscan -p "$PORT" -t ed25519 "$HOST" 2>/dev/null > "$scan"
  local got want
  got=$(ssh-keygen -lf "$scan" | awk '{print $2}')
  want=$(ssh-keygen -lf "$PIN" | awk '{print $2}')
  echo "keyscan ed25519: $got"
  echo "pinned         : $want"
  [ -n "$got" ] && [ "$got" = "$want" ] || { echo "FINGERPRINT MISMATCH: stop" >&2; exit 3; }
  echo "== ssh whoami (hardened options)"
  ssh "${SSH_ARGS[@]}" -- "$DEST" whoami < /dev/null | tr -d '\r'
  echo "== ign status"
  "$IGN" --profile "$PROFILE" --json status | jq -c '{ok, state: .data.state, name: .data.gateway.name, version: .data.gateway.ignition_version, license: .data.gateway.license}'
  echo "== deploymentMode"
  "$IGN" --profile "$PROFILE" --json api call --method GET --path /data/api/v1/gateway-info | jq -c '.data.result.data | {name, deploymentMode, ignitionVersion, license}' 
}

# ===========================================================================
c7a() {
  refuse_if_present 'C7a'
  sect 'C7a — Windows OpenSSH behaviours as svc-ignition-deploy (D-13, D-15)'
  prose "Capture host: $HOST, an Azure VM (Windows Server 2022) standing in for WFLWVM01 by user decision of 2026-10-14. Account: \`$SSH_USER\`. Every command runs from the macOS runner with the hardened option set of \`hardened_options\`; \`<key>\` is the temp key file. \`ssh\` is ssh -T with the remote command as one argument; \`sftp\` is \`sftp -q -b -\` with the batch on stdin. The Windows-client ssh.exe stdin item of the C7a procedure is NOT applicable to a macOS runner; it is exercised by the windows-latest CI leg (22-13)."
  h_def

  sub 'C7a-host — account, shell, groups, host key types'
  rcap 'whoami' 'whoami'
  rcap 'hostname' 'hostname'
  rcap 'ver' 'ver'
  rcap 'default shell: COMSPEC and the registry value' 'echo COMSPEC=%COMSPEC% & reg query HKLM\SOFTWARE\OpenSSH /v DefaultShell'
  rcap 'USERPROFILE' 'echo %USERPROFILE%'
  rcap 'the account groups (whoami /groups)' 'whoami /groups'
  rcap 'the account privileges (whoami /priv)' 'whoami /priv'
  rcap 'is the account an administrator? (net localgroup Administrators)' 'net localgroup Administrators'
  pscap 'PowerShell version on the host' '$PSVersionTable.PSVersion.ToString()' prelude
  run_block 'host key types offered: ssh-keyscan -t ed25519,rsa,ecdsa (fingerprints via ssh-keygen -lf -)' "" bash -c 'ssh-keyscan -p "$1" -t ed25519,rsa,ecdsa "$2" 2>&1 | tee /dev/stderr | ssh-keygen -lf - 2>&1' _ "$PORT" "$HOST"
  run_block 'host key types the server offers in KEX (ssh -vv, algorithm lines only; LogLevel option left out of this one block so -vv shows)' "" bash -c '
    ssh -vv -T -p "$1" -i "$2" -o BatchMode=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$3" -o GlobalKnownHostsFile=/dev/null -o IdentitiesOnly=yes -o ConnectTimeout=10 -- "$4" exit 0 2>&1 </dev/null | grep -E "host key algorithms|kex algorithms|Server host key|Remote protocol version" | cut -c1-300' _ "$PORT" "$KEYFILE" "$PIN" "$DEST"

  local home_abs home_win
  home_abs=$(sq 'pwd' | sed -n 's/^Remote working directory: //p')
  home_win=$(win_home)
  [ -n "$home_abs" ] || { echo "could not read the sftp home" >&2; exit 4; }
  local H="$home_abs/ign-c7a"
  local Hdrive=${home_abs#/}
  local Hbs=${Hdrive//\//\\}
  prose "Home as seen by sftp: \`$home_abs\`; as seen by cmd.exe: \`$home_win\`. All scratch work is under \`ign-c7a/\` in that home and is removed at the end."

  printf 'alpha\n' > "$LOC/a.txt"
  printf 'bravo\n' > "$LOC/b.txt"
  printf 'charlie\n' > "$LOC/c.txt"
  printf 'unicode\n' > "$LOC/u.txt"

  sub 'C7a-1 — sftp path forms'
  scap 'pwd and a listing of the login directory' 'pwd' '@ls -1 "."'
  scap 'create the scratch directory (relative path)' 'mkdir "ign-c7a"' 'mkdir "ign-c7a/d"' "@ls -1 \"ign-c7a\""
  local f
  for f in \
    "$RES_SFTP" \
    "C:/Program Files/Inductive Automation/Ignition/data/config/resources" \
    "/c:/program files/inductive automation/ignition/data/config/resources" \
    "/C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources" \
    "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources" \
    "/Program Files/Inductive Automation/Ignition/data/config/resources" \
    "$RES_SFTP/" ; do
    scap "ls -1 with the path form: $f" "@ls -1 \"$f\""
  done
  scap 'ls -1 of the drive root forms' '@ls -1 "/C:/"' ; scap 'ls -1 of "/"' '@ls -1 "/"'
  scap 'ls -1 of "C:/" (no leading slash)' '@ls -1 "C:/"'

  # a small directory to fetch with each form
  scap 'what core/ignition holds (to pick a small directory for get -R)' "@ls -1 \"$RES_SFTP/core/ignition\""
  local small="$RES_SFTP/core/ignition/security-properties"
  scap 'ls -l of the small directory chosen for get -R' "@ls -l \"$small\""
  local n=0 g
  for g in \
    "$small" \
    "${small#/}" \
    "$(printf '%s' "${small#/}" | tr '/' '\\')" ; do
    n=$((n + 1))
    mkdir -p "$LOC/get$n"
    scap "get -R with the path form: $g" "get -R \"$g\" \"$LOC/get$n\""
    run_block "what get -R left locally (find, sizes, sha256 of each file)" "" bash -c 'cd "$1" && find . | LC_ALL=C sort && find . -type f -exec shasum -a 256 {} \;' _ "$LOC/get$n"
  done

  scap 'put with the relative path form' "put \"$LOC/a.txt\" \"ign-c7a/put-rel.txt\""
  scap 'put with the /C:/ absolute path form' "put \"$LOC/a.txt\" \"$H/put-slashc.txt\""
  scap 'put with the C:/ drive path form (no leading slash)' "put \"$LOC/a.txt\" \"$Hdrive/ign-c7a/put-drive.txt\""
  scap 'put with backslashes: ign-c7a\put-bs.txt' "put \"$LOC/a.txt\" \"ign-c7a\\put-bs.txt\""
  scap 'put with an absolute backslash drive path' "put \"$LOC/a.txt\" \"$Hbs\\ign-c7a\\put-bs2.txt\""
  scap 'what exists after the puts (ls -1)' '@ls -1 "ign-c7a"'
  rcap 'what exists after the puts (dir /b from the cmd.exe side)' 'dir /b "%USERPROFILE%\ign-c7a"'

  scap 'rename, relative to relative' "put \"$LOC/b.txt\" \"ign-c7a/r1.txt\"" 'rename "ign-c7a/r1.txt" "ign-c7a/r1-renamed.txt"' '@ls -1 "ign-c7a"'
  scap 'rename, /C:/ absolute to /C:/ absolute' "rename \"$H/r1-renamed.txt\" \"$H/r1-abs.txt\"" '@ls -1 "ign-c7a"'
  scap 'rename, relative source to absolute /C:/ target' "rename \"ign-c7a/r1-abs.txt\" \"$H/r1-mixed.txt\"" '@ls -1 "ign-c7a"'
  scap 'rename into a subdirectory (same volume)' "rename \"ign-c7a/r1-mixed.txt\" \"ign-c7a/d/r1-moved.txt\"" '@ls -1 "ign-c7a/d"'
  scap 'rename with backslash paths' 'rename "ign-c7a\d\r1-moved.txt" "ign-c7a\d\r1-bs.txt"' '@ls -1 "ign-c7a/d"'

  sub 'C7a-2 — listing format (ls -1, ls -l, ls -la), paths with spaces and commas'
  scap 'create a directory with spaces and a comma, files with spaces, commas and non-ASCII' \
    'mkdir "ign-c7a/dir with space, comma"' \
    "put \"$LOC/a.txt\" \"ign-c7a/dir with space, comma/f one.txt\"" \
    "put \"$LOC/b.txt\" \"ign-c7a/dir with space, comma/f,two.txt\"" \
    "put \"$LOC/u.txt\" \"ign-c7a/dir with space, comma/caf${LC_E:-é}.txt\"" \
    'mkdir "ign-c7a/dir with space, comma/sub dir"'
  scap 'ls -1 of the spaces-and-comma directory' '@ls -1 "ign-c7a/dir with space, comma"'
  scap 'ls -l of the spaces-and-comma directory' '@ls -l "ign-c7a/dir with space, comma"'
  scap 'ls -la of the spaces-and-comma directory' '@ls -la "ign-c7a/dir with space, comma"'
  run_block 'the same ls -l with the client in a UTF-8 locale (LC_ALL=en_US.UTF-8): does the sftp client print the non-ASCII name raw?' "@ls -l \"ign-c7a/dir with space, comma\"" env LC_ALL=en_US.UTF-8 sftp "${SFTP_ARGS[@]}" -- "$DEST"
  mkdir -p "$LOC/spaceget"
  run_block 'get -R of the spaces-and-comma directory with the client in a UTF-8 locale (local names)' "get -R \"ign-c7a/dir with space, comma\" \"$LOC/spaceget\"" env LC_ALL=en_US.UTF-8 sftp "${SFTP_ARGS[@]}" -- "$DEST"
  run_block 'what that get -R left locally (names as the local filesystem holds them, sha256)' "" bash -c 'cd "$1" && find . | LC_ALL=C sort && find . -type f -exec shasum -a 256 {} \;' _ "$LOC/spaceget"
  scap 'ls -1 with the absolute /C:/ form of the same directory' "@ls -1 \"$H/dir with space, comma\""
  scap 'ls -l of a single file' '@ls -l "ign-c7a/dir with space, comma/f one.txt"'
  scap 'ls -1 of a single file' '@ls -1 "ign-c7a/dir with space, comma/f one.txt"'
  scap 'ls -1 of an empty directory' 'mkdir "ign-c7a/empty"' '@ls -1 "ign-c7a/empty"'
  scap 'ls -la of an empty directory (. and .. entries?)' '@ls -la "ign-c7a/empty"'
  rcap 'the same directory from the cmd.exe side (dir /b, dir /a)' 'dir /a "%USERPROFILE%\ign-c7a\dir with space, comma"'
  scap 'ls -l of the real collection root (owners, permissions, size, date columns)' "@ls -l \"$RES_SFTP\""
  scap 'ls -la of the real collection root' "@ls -la \"$RES_SFTP\""

  sub 'C7a-3 — error signalling: sftp -b - exit codes and stderr'
  local ADMIN_PROFILE
  ADMIN_PROFILE=$(rq 'dir /b C:\Users' | grep -viE '^(svc-ignition-deploy|Public|Default|Default User|All Users|desktop.ini)$' | head -n1 | tr -d '\r\n' || true)
  scap 'not found: ls -1' '@ls -1 "ign-c7a/does-not-exist"'
  scap 'not found: get' "get \"ign-c7a/does-not-exist\" \"$LOC/nf.txt\""
  scap 'not found: get -R' "get -R \"ign-c7a/does-not-exist\" \"$LOC\""
  scap 'not found: rm' 'rm "ign-c7a/does-not-exist"'
  scap 'not found: rmdir' 'rmdir "ign-c7a/does-not-exist"'
  scap 'not found: rename source' 'rename "ign-c7a/does-not-exist" "ign-c7a/xx"'
  scap 'rename target parent missing' "put \"$LOC/c.txt\" \"ign-c7a/np.txt\"" 'rename "ign-c7a/np.txt" "ign-c7a/no-such-dir/np.txt"'
  scap 'mkdir of an existing directory' 'mkdir "ign-c7a/d"'
  scap 'rmdir of a non-empty directory' 'rmdir "ign-c7a/dir with space, comma"'
  scap 'permission denied: put into the resources root' "put \"$LOC/a.txt\" \"$RES_SFTP/zz-c7a-denied.txt\""
  scap 'permission denied: mkdir in the resources root' "mkdir \"$RES_SFTP/zz-c7a-denied\""
  scap 'what the data dir holds (ls -1)' "@ls -1 \"$DATA_SFTP\""
  rcap 'user profile directories on the host (dir /b C:\Users)' 'dir /b C:\Users'
  scap "permission denied: ls of another account's profile directory (${ADMIN_PROFILE:-none found})" "@ls -1 \"/C:/Users/${ADMIN_PROFILE:-none}\""
  scap 'permission denied: ls of C:/Windows/System32/config' '@ls -1 "/C:/Windows/System32/config"'
  scap 'permission denied: ls -l of the sshd host private key file (metadata only; the file is never fetched)' '@ls -l "/C:/ProgramData/ssh/ssh_host_ed25519_key"'
  scap 'permission denied: get of the sshd log (a file that exists and is not readable by Users)' "get \"/C:/ProgramData/ssh/logs/sshd.log\" \"$LOC/sshd.log\""
  scap 'permission denied: get of the SAM hive' "get \"/C:/Windows/System32/config/SAM\" \"$LOC/sam\""
  rcap 'after the denied writes: nothing was created in the resources root' "dir /b \"$RES_WIN\" | findstr /i zz-c7a"
  scap 'sftp exit code with a failing command in the MIDDLE of a batch (does the batch abort?)' '@ls -1 "ign-c7a/d"' '@ls -1 "ign-c7a/does-not-exist"' '@ls -1 "ign-c7a/empty"'
  scap 'sftp batch whose commands all succeed (exit code 0)' '@ls -1 "ign-c7a/d"' '@ls -1 "ign-c7a/empty"'

  sub 'C7a-4 — rename over an existing target'
  scap 'file over an existing file: setup' "put \"$LOC/a.txt\" \"ign-c7a/ow-src.txt\"" "put \"$LOC/b.txt\" \"ign-c7a/ow-dst.txt\"" '@ls -l "ign-c7a/ow-src.txt"' '@ls -l "ign-c7a/ow-dst.txt"'
  scap 'file over an existing file: rename ow-src.txt onto ow-dst.txt' 'rename "ign-c7a/ow-src.txt" "ign-c7a/ow-dst.txt"'
  rcap 'file over an existing file: what is left, and the content of ow-dst.txt (alpha means replaced, bravo means untouched)' 'dir /b "%USERPROFILE%\ign-c7a\ow-*" & type "%USERPROFILE%\ign-c7a\ow-dst.txt"'
  scap 'directory over an existing EMPTY directory' 'mkdir "ign-c7a/od-src"' "put \"$LOC/a.txt\" \"ign-c7a/od-src/in-src.txt\"" 'mkdir "ign-c7a/od-dst-empty"' 'rename "ign-c7a/od-src" "ign-c7a/od-dst-empty"'
  rcap 'directory over an empty directory: the whole tree (is in-src.txt inside od-dst-empty?)' 'cmd /c "dir /b /s "%USERPROFILE%\ign-c7a" | findstr /i /c:"\od-""'
  scap 'directory over an existing NON-EMPTY directory: setup' 'mkdir "ign-c7a/od2-src"' "put \"$LOC/a.txt\" \"ign-c7a/od2-src/in-src.txt\"" 'mkdir "ign-c7a/od2-dst"' "put \"$LOC/b.txt\" \"ign-c7a/od2-dst/in-dst.txt\""
  scap 'directory over an existing NON-EMPTY directory: rename' 'rename "ign-c7a/od2-src" "ign-c7a/od2-dst"'
  rcap 'directory over a non-empty directory: the whole tree (both directories intact?)' 'cmd /c "dir /b /s "%USERPROFILE%\ign-c7a" | findstr /i /c:"\od2-""'
  scap 'file over an existing DIRECTORY' "put \"$LOC/a.txt\" \"ign-c7a/fd-src.txt\"" 'mkdir "ign-c7a/fd-dst"' 'rename "ign-c7a/fd-src.txt" "ign-c7a/fd-dst"'
  scap 'rename onto itself, step 1: a fresh file and its listing' "put \"$LOC/a.txt\" \"ign-c7a/self.txt\"" '@ls -l "ign-c7a/self.txt"'
  scap 'rename onto itself, step 2: rename self.txt onto self.txt' 'rename "ign-c7a/self.txt" "ign-c7a/self.txt"'
  scap 'rename onto itself, step 3: is the file still there?' '@ls -l "ign-c7a/self.txt"'
  rcap 'rename onto itself, step 4: the file from the cmd.exe side' 'dir /b "%USERPROFILE%\ign-c7a\self.txt" & type "%USERPROFILE%\ign-c7a\self.txt"'
  scap 'rename over an existing file twice in a row (the commit step may repeat): first' "put \"$LOC/a.txt\" \"ign-c7a/tw-src1.txt\"" "put \"$LOC/b.txt\" \"ign-c7a/tw-dst.txt\"" 'rename "ign-c7a/tw-src1.txt" "ign-c7a/tw-dst.txt"'
  scap 'rename over an existing file twice in a row: second' "put \"$LOC/c.txt\" \"ign-c7a/tw-src2.txt\"" 'rename "ign-c7a/tw-src2.txt" "ign-c7a/tw-dst.txt"' '@ls -1 "ign-c7a"'
  rcap 'rename over an existing file twice in a row: content of tw-dst.txt (charlie means the second replaced the first)' 'type "%USERPROFILE%\ign-c7a\tw-dst.txt"'
  scap 'rename over a file that another session holds open: setup' "put \"$LOC/b.txt\" \"ign-c7a/lock-dst.txt\""
  run_block 'rename over a file that a second ssh session holds open with FileShare.None (4 s hold; the sftp rename starts 2 s after it)' "" bash -c '
    HOLD=$(printf "%s" "\$f=[IO.File]::Open((Join-Path \$env:USERPROFILE \"ign-c7a\\lock-dst.txt\"),\"Open\",\"Read\",\"None\"); \"held\"; Start-Sleep 4; \$f.Close(); \"released\"" | iconv -f UTF-8 -t UTF-16LE | base64 | tr -d "\n")
    ssh -T -p "$1" -i "$2" -o BatchMode=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$3" -o GlobalKnownHostsFile=/dev/null -o IdentitiesOnly=yes -o LogLevel=ERROR -o ConnectTimeout=10 -- "$4" "powershell -NoProfile -NonInteractive -EncodedCommand $HOLD" </dev/null 2>/dev/null &
    sleep 2
    printf "%s\n" "put \"$5\" \"ign-c7a/lock-src.txt\"" "rename \"ign-c7a/lock-src.txt\" \"ign-c7a/lock-dst.txt\"" | sftp -q -b - -P "$1" -i "$2" -o BatchMode=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$3" -o GlobalKnownHostsFile=/dev/null -o IdentitiesOnly=yes -o LogLevel=ERROR -o ConnectTimeout=10 -- "$4"
    echo "sftp-exit=$?"
    wait' _ "$PORT" "$KEYFILE" "$PIN" "$DEST" "$LOC/c.txt"
  rcap 'after the held-handle rename attempt: which names exist, and the content of lock-dst.txt' 'dir /b "%USERPROFILE%\ign-c7a\lock-*" & type "%USERPROFILE%\ign-c7a\lock-dst.txt"'

  sub 'C7a-5 — junctions and links'
  rcap 'create jx/real, a file in it, and a junction jx/junc to it (cmd /c mklink /J)' 'cmd /c "mkdir "%USERPROFILE%\ign-c7a\jx\real" && echo inreal> "%USERPROFILE%\ign-c7a\jx\real\r.txt" && mklink /J "%USERPROFILE%\ign-c7a\jx\junc" "%USERPROFILE%\ign-c7a\jx\real""'
  rcap 'dir of jx (junction marker)' 'dir /a "%USERPROFILE%\ign-c7a\jx"'
  scap 'ls -la of jx (how the junction appears to sftp)' '@ls -la "ign-c7a/jx"'
  scap 'ls -l of jx/junc contents' '@ls -l "ign-c7a/jx/junc"'
  scap 'get of a file reached through the junction' "get \"ign-c7a/jx/junc/r.txt\" \"$LOC/viajunc.txt\""
  mkdir -p "$LOC/jget"
  scap 'get -R of jx (contains the junction)' "get -R \"ign-c7a/jx\" \"$LOC/jget\""
  run_block 'what get -R over a junction left locally' "" bash -c 'cd "$1" && find . | LC_ALL=C sort && find . -type f -exec shasum -a 256 {} \;' _ "$LOC/jget"
  rcap 'a junction that points at its own parent (jx/loop -> jx)' 'cmd /c mklink /J "%USERPROFILE%\ign-c7a\jx\loop" "%USERPROFILE%\ign-c7a\jx"'
  mkdir -p "$LOC/jloop"
  scap 'get -R of jx WITH the self-referencing junction (150 s alarm bounds a runaway)' "get -R \"ign-c7a/jx\" \"$LOC/jloop\""
  run_block 'what the looping get -R left locally: entry count and deepest path length (not the listing)' "" bash -c 'cd "$1" && echo "entries=$(find . | wc -l | tr -d " ")" && echo "deepest_path_chars=$(find . | awk "{ if (length(\$0) > m) m = length(\$0) } END { print m }")" && find . -type f | head -n 5' _ "$LOC/jloop"
  rcap 'a directory symbolic link (mklink /D), which needs a privilege the account may not hold' 'cmd /c mklink /D "%USERPROFILE%\ign-c7a\jx\symd" "%USERPROFILE%\ign-c7a\jx\real"'
  scap 'ls -la of jx after the symlink attempt' '@ls -la "ign-c7a/jx"'
  rcap 'remove the junctions (rmdir removes the link only)' 'cmd /c "rmdir "%USERPROFILE%\ign-c7a\jx\loop" & rmdir "%USERPROFILE%\ign-c7a\jx\junc" & rmdir "%USERPROFILE%\ign-c7a\jx\symd" & dir /a "%USERPROFILE%\ign-c7a\jx" & dir /b "%USERPROFILE%\ign-c7a\jx\real""'

  sub 'C7a-6 — default shell and PowerShell -EncodedCommand: exit codes and CLIXML'
  pscap 'ok output, bare' "'ok'"
  pscap 'ok output, with the prelude' "'ok'" prelude
  pscap 'exit 7, bare' 'exit 7'
  pscap 'exit 7, with the prelude' 'exit 7' prelude
  pscap 'throw, bare' "throw 'boom'"
  pscap 'throw, with the prelude' "throw 'boom'" prelude
  pscap 'a cmdlet error (Get-Item of a missing path), bare' "Get-Item 'C:\\definitely-missing'"
  pscap 'a cmdlet error (Get-Item of a missing path), with the prelude' "Get-Item 'C:\\definitely-missing'" prelude
  pscap 'Write-Error then continue, bare' "Write-Error 'e1'; 'after'"
  pscap 'Write-Error then continue, with the prelude' "Write-Error 'e1'; 'after'" prelude
  pscap 'Write-Progress, bare' "Write-Progress -Activity a -Status b; 'done'"
  pscap 'Write-Progress, with the prelude' "Write-Progress -Activity a -Status b; 'done'" prelude
  pscap 'real progress: Compress-Archive of a small file then Expand-Archive, bare' "\$d=Join-Path \$env:USERPROFILE 'ign-c7a'; Compress-Archive -LiteralPath (Join-Path \$d 'put-rel.txt') -DestinationPath (Join-Path \$d 'z.zip') -Force; Expand-Archive -LiteralPath (Join-Path \$d 'z.zip') -DestinationPath (Join-Path \$d 'zx') -Force; Remove-Item -LiteralPath (Join-Path \$d 'z.zip'),(Join-Path \$d 'zx') -Recurse -Force; 'archived'"
  pscap 'real progress: Compress-Archive of a small file then Expand-Archive, with the prelude' "\$d=Join-Path \$env:USERPROFILE 'ign-c7a'; Compress-Archive -LiteralPath (Join-Path \$d 'put-rel.txt') -DestinationPath (Join-Path \$d 'z.zip') -Force; Expand-Archive -LiteralPath (Join-Path \$d 'z.zip') -DestinationPath (Join-Path \$d 'zx') -Force; Remove-Item -LiteralPath (Join-Path \$d 'z.zip'),(Join-Path \$d 'zx') -Recurse -Force; 'archived'" prelude
  pscap 'a stderr line written by the host, with the prelude' "[Console]::Error.WriteLine('to-stderr'); 'to-stdout'" prelude
  pscap 'non-ASCII output (UTF-8 or the console code page?), with the prelude' "'caf$(printf '\xc3\xa9') ü ñ'" prelude
  pscap 'a script that prints a sentinel line and a JSON object, with the prelude' "Write-Output 'IGN-RESULT {\"ok\":true}'" prelude
  pscap 'PowerShell Rename-Item onto an existing name (src and dst both exist)' "\$d=Join-Path \$env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path \$d 'pa-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path \$d 'pa-dst.txt') -Value 'old'; Rename-Item -LiteralPath (Join-Path \$d 'pa-src.txt') -NewName 'pa-dst.txt'; Get-Content -LiteralPath (Join-Path \$d 'pa-dst.txt')" prelude
  pscap 'PowerShell Move-Item -Force onto an existing name' "\$d=Join-Path \$env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path \$d 'pb-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path \$d 'pb-dst.txt') -Value 'old'; Move-Item -LiteralPath (Join-Path \$d 'pb-src.txt') -Destination (Join-Path \$d 'pb-dst.txt') -Force; Get-Content -LiteralPath (Join-Path \$d 'pb-dst.txt')" prelude
  pscap '.NET File.Move onto an existing name (.NET Framework 4.x has no overwrite)' "\$d=Join-Path \$env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path \$d 'pc-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path \$d 'pc-dst.txt') -Value 'old'; [IO.File]::Move((Join-Path \$d 'pc-src.txt'),(Join-Path \$d 'pc-dst.txt')); Get-Content -LiteralPath (Join-Path \$d 'pc-dst.txt')" prelude
  pscap '.NET File.Replace with a null backup name (atomic swap of two files)' "\$d=Join-Path \$env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path \$d 'pd-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path \$d 'pd-dst.txt') -Value 'old'; [IO.File]::Replace((Join-Path \$d 'pd-src.txt'),(Join-Path \$d 'pd-dst.txt'),[NullString]::Value); Get-Content -LiteralPath (Join-Path \$d 'pd-dst.txt')" prelude
  pscap 'Directory swap: Move-Item of a directory onto an existing directory name, and rename-aside-then-rename (the directory commit sequence)' "\$d=Join-Path \$env:USERPROFILE 'ign-c7a'; New-Item -ItemType Directory -Path (Join-Path \$d 'pe-new') | Out-Null; Set-Content -LiteralPath (Join-Path \$d 'pe-new\\n.txt') -Value 'new'; New-Item -ItemType Directory -Path (Join-Path \$d 'pe-live') | Out-Null; Set-Content -LiteralPath (Join-Path \$d 'pe-live\\o.txt') -Value 'old'; Rename-Item -LiteralPath (Join-Path \$d 'pe-live') -NewName 'pe-old'; Rename-Item -LiteralPath (Join-Path \$d 'pe-new') -NewName 'pe-live'; (Get-ChildItem -LiteralPath (Join-Path \$d 'pe-live')).Name; (Get-ChildItem -LiteralPath (Join-Path \$d 'pe-old')).Name" prelude
  rcap 'ssh with a plain cmd.exe command: exit code propagation' 'cmd /c exit 5'
  rcap 'ssh with a plain command that does not exist: exit code and stderr' 'definitely-not-a-command-xyz'
  rcap 'ssh with no output and exit 0' 'cmd /c exit 0'

  sub 'C7a-7 — maximum command length'
  ladder_block

  sub 'C7a-8 — sftp-server paths over 260 characters'
  longpath_blocks "$home_abs" "$home_win"

  sub 'C7a-cleanup'
  scratch_clean
  rcap 'the scratch directory is gone (dir of the home)' 'dir /b "%USERPROFILE%"'
}

# One block: command length ladder. Every row is its own run, stamped.
ladder_block() {
  local targets="1000 4000 8000 8100 8120 8140 8150 8160 8170 8180 8185 8190 8191 8192 8300 12000 24000 32000 32700 32760 32766 32767 32768 33000 65000 100000"
  local started so=$TMPD/lad
  started=$(utc)
  {
    printf '\n#### command-length ladder: powershell -EncodedCommand with a padded script and the prelude; each row is one ssh run\n\n'
    printf '`script = "<prelude>$x=\x27<pad>\x27; Write-Output (\x27LEN=\x27 + $x.Length)"`, padding chosen so the REMOTE COMMAND STRING (the argument after the destination) is about T characters; each row runs `ssh -T -p 22 -i <key> <H> -- user@host <that string>` @ %s\n\n~~~\n' "$started"
    printf 'target_chars  actual_chars  exit  seconds  stdout(first line)  stderr(first 120 chars)\n'
  } >> "$SEC"
  local t pad n script cmd rc t0 t1 line1 err1
  for t in $targets; do
    # command = 54 prefix chars + base64(UTF-16LE(script)); base64 grows 4/3 per 2 bytes
    n=$(( (t - 54) * 3 / 8 ))
    pad=$(head -c $((n - 110)) /dev/zero | tr '\0' 'A')
    script="$PS_PRELUDE\$x='$pad'; Write-Output ('LEN=' + \$x.Length)"
    cmd=$(ps_command "$script")
    t0=$(date +%s)
    rc=0
    perl -e 'alarm 150; exec @ARGV' ssh "${SSH_ARGS[@]}" -- "$DEST" "$cmd" < /dev/null > "$so" 2> "$so.e" || rc=$?
    t1=$(date +%s)
    line1=$(head -n1 "$so" | tr -d '\r' | cut -c1-60)
    err1=$(head -c 120 "$so.e" | tr -d '\r\n')
    printf '%12s  %12s  %4s  %7s  %s  %s\n' "$t" "${#cmd}" "$rc" "$((t1 - t0))" "$line1" "$err1" >> "$SEC"
  done
  printf '~~~\n' >> "$SEC"
}

# Long paths: build nesting through sftp mkdir/cd, then through PowerShell with
# the \\?\ prefix when sftp stops short, then read the deep tree back.
longpath_blocks() {
  local home_abs=$1
  local seg
  seg=$(printf 'x%.0s' $(seq 1 56))
  local lines=() i
  lines+=('mkdir "ign-c7a/lp"' 'cd "ign-c7a/lp"')
  for i in 1 2 3 4 5 6 7; do
    lines+=("mkdir \"s${i}${seg}\"" "cd \"s${i}${seg}\"")
  done
  lines+=("put \"$LOC/a.txt\" \"deep.txt\"" 'pwd' '@ls -1 "."')
  scap 'build seven nested directories of 58 characters by mkdir + cd, then put a file (the batch aborts at the first failing command; the echoed lines show how far it got)' "${lines[@]}"
  pscap 'longest full path under ign-c7a/lp as PowerShell sees it (\\?\ prefix; length in characters)' "\$p='\\\\?\\'+(Join-Path \$env:USERPROFILE 'ign-c7a\\lp'); \$m=0; \$last=''; Get-ChildItem -LiteralPath \$p -Recurse -Force | ForEach-Object { \$l=\$_.FullName.Length; if(\$l -gt \$m){ \$m=\$l; \$last=\$_.FullName } }; 'max_len=' + \$m; \$last" prelude

  # Independent of how far sftp got: make a tree past 260 with \\?\ and read it.
  pscap 'build a tree with a 300+ character path with the \\?\ prefix (PowerShell), put a file at the bottom' "\$base='\\\\?\\'+(Join-Path \$env:USERPROFILE 'ign-c7a\\lq'); \$seg='y'*56; \$p=\$base; 1..6 | ForEach-Object { \$p=\$p+'\\'+('t'+\$_+\$seg) }; New-Item -ItemType Directory -Path \$p -Force | Out-Null; Set-Content -LiteralPath (\$p+'\\deep.txt') -Value 'deep'; 'bottom_len=' + (\$p+'\\deep.txt').Length" prelude
  local deep="$home_abs/ign-c7a/lq" j
  for j in 1 2 3 4 5 6; do deep="$deep/t${j}$(printf 'y%.0s' $(seq 1 56))"; done
  local lvl="$home_abs/ign-c7a/lq" k
  for k in 1 2 3 4 5 6; do
    lvl="$lvl/t${k}$(printf 'y%.0s' $(seq 1 56))"
    scap "ls -1 of level $k of the deep tree, absolute path of ${#lvl} characters" "@ls -1 \"$lvl\""
  done
  scap "ls -l of the deepest directory by its absolute path (${#deep} chars)" "@ls -l \"$deep\""
  mkdir -p "$LOC/deepget"
  scap "get of the deep file by its absolute path" "get \"$deep/deep.txt\" \"$LOC/deepget/deep.txt\""
  scap "get -R of ign-c7a/lq (the whole deep tree)" "get -R \"ign-c7a/lq\" \"$LOC/deepget\""
  run_block 'what the deep get -R left locally: every directory and file with its path length (a silent stop would show as missing levels)' "" bash -c 'cd "$1" && find . | awk "{ print length(\$0) \" \" \$0 }" | cut -c1-140 && find . -type f -exec shasum -a 256 {} \;' _ "$LOC/deepget"
  pscap 'what the deep tree really holds (levels and the file) as PowerShell sees it' "\$p='\\\\?\\'+(Join-Path \$env:USERPROFILE 'ign-c7a\\lq'); Get-ChildItem -LiteralPath \$p -Recurse -Force | ForEach-Object { '{0} {1}' -f \$_.FullName.Length, (\$_.FullName.Substring(\$_.FullName.Length - [Math]::Min(30, \$_.FullName.Length))) }" prelude
  scap 'ls -1 of the intermediate levels (is the path limit per call or per tree?)' "@ls -1 \"$home_abs/ign-c7a/lq\"" "@ls -1 \"$home_abs/ign-c7a/lq/t1$(printf 'y%.0s' $(seq 1 56))\""
}

# ===========================================================================
c6() {
  refuse_if_present 'C6'
  sect 'C6 — what svc-ignition-deploy can write and read (Decision B, research A16)'
  prose "Account \`$SSH_USER\` on $HOST. The gateway is the one behind the \`$PROFILE\` profile. A write probe inside the mode collection uses a new directory \`zz-c6-probe\` that is removed in the same session; \`GET /data/api/v1/scan/config\` is recorded before and after. Home and \`ign-deploy\` probes are removed at the end. ACL listings are taken as the account over ssh (\`icacls\`), the account's own view."
  h_def
  local home_abs home_win
  home_abs=$(sq 'pwd' | sed -n 's/^Remote working directory: //p')
  home_win=$(win_home)
  [ -n "$home_abs" ] || { echo "could not read the sftp home" >&2; exit 4; }
  printf 'alpha\n' > "$LOC/a.txt"
  printf 'bravo\n' > "$LOC/b.txt"

  sub 'C6-acl — the ACLs that explain each answer'
  rcap 'whoami /groups (the account, again)' 'whoami /groups'
  rcap 'icacls data dir' "icacls \"$DATA_WIN\""
  rcap 'icacls data\\config' "icacls \"$DATA_WIN\\config\""
  rcap 'icacls data\\config\\resources' "icacls \"$RES_WIN\""
  rcap 'icacls data\\config\\resources\\core' "icacls \"$RES_WIN\\core\""
  rcap 'dir of data\\config\\resources (which collections exist)' "dir /a \"$RES_WIN\""
  rcap 'icacls data\\config\\resources\\production' "icacls \"$RES_WIN\\production\""
  rcap 'icacls data\\ign-deploy (does it exist?)' "icacls \"$DATA_WIN\\ign-deploy\""
  rcap 'icacls the account home' 'icacls "%USERPROFILE%"'
  rcap 'dir of the data dir' "dir /a \"$DATA_WIN\""
  rcap 'is the data dir and the home on the same volume? (fsutil is admin-only; vol and the drive letters instead)' 'echo %USERPROFILE% & echo %SystemDrive% & vol C:'

  sub 'C6-scan-before — gateway scan state before any write probe'
  icap 'GET scan/config (before)' api call --method GET --path /data/api/v1/scan/config
  icap 'GET mode (deployment modes)' api call --method GET --path /data/api/v1/mode
  icap 'GET gateway-info (the active deploymentMode)' api call --method GET --path /data/api/v1/gateway-info

  sub 'C6-1 — home-relative staging directory ign-deploy/'
  scap 'mkdir ign-deploy in the home, put a file, rename it, list it' \
    'mkdir "ign-deploy"' 'mkdir "ign-deploy/gw"' "put \"$LOC/a.txt\" \"ign-deploy/gw/stage1.txt\"" \
    'rename "ign-deploy/gw/stage1.txt" "ign-deploy/gw/stage2.txt"' '@ls -l "ign-deploy/gw"'
  scap 'the same under the /C:/ absolute home path' "put \"$LOC/b.txt\" \"$home_abs/ign-deploy/gw/abs.txt\"" "@ls -1 \"$home_abs/ign-deploy/gw\""

  sub 'C6-2 — write directly into the mode collection (production)'
  local P="$RES_SFTP/production"
  scap 'ls -la of production (does the collection directory exist, who owns it)' "@ls -la \"$P\""
  scap 'mkdir zz-c6-probe inside production' "mkdir \"$P/zz-c6-probe\""
  scap 'cross-directory rename of a staged FILE directly into production (the B(i) commit step; no probe directory needed)' "rename \"ign-deploy/gw/stage2.txt\" \"$P/zz-c6-moved.txt\""
  scap 'cross-directory rename of a staged DIRECTORY directly into production' 'mkdir "ign-deploy/gw/unit"' "put \"$LOC/a.txt\" \"ign-deploy/gw/unit/f.txt\"" "rename \"ign-deploy/gw/unit\" \"$P/zz-c6-unit\""
  scap 'put a file straight into production (no subdirectory)' "put \"$LOC/a.txt\" \"$P/zz-c6-direct.txt\""
  scap 'put a file into production/zz-c6-probe' "put \"$LOC/a.txt\" \"$P/zz-c6-probe/probe.txt\""
  scap 'rename inside production/zz-c6-probe' "rename \"$P/zz-c6-probe/probe.txt\" \"$P/zz-c6-probe/probe2.txt\""
  scap 'cross-directory rename from the home staging dir INTO production/zz-c6-probe (the B(i) commit step)' \
    "rename \"ign-deploy/gw/stage2.txt\" \"$P/zz-c6-probe/moved.txt\""
  scap 'ls -l of production/zz-c6-probe after the attempts' "@ls -l \"$P/zz-c6-probe\""
  scap 'remove the probe: rm the files, rmdir the directory' \
    "rm \"$P/zz-c6-probe/probe.txt\"" "rm \"$P/zz-c6-probe/probe2.txt\"" "rm \"$P/zz-c6-probe/moved.txt\"" "rmdir \"$P/zz-c6-probe\""
  scap 'production after the cleanup (the probe directory must be gone)' "@ls -1 \"$P\""
  scap 'the same probes against the collection root itself: mkdir zz-c6-root-probe directly under resources' "mkdir \"$RES_SFTP/zz-c6-root-probe\""
  scap 'mkdir the collection directory production2 under resources (what creating a new collection would need)' "mkdir \"$RES_SFTP/zz-c6-collection\""

  sub 'C6-3 — write <data_dir>\ign-deploy\'
  scap 'mkdir ign-deploy directly under the data dir' "mkdir \"$DATA_SFTP/ign-deploy\""
  scap 'mkdir ign-deploy under data/config' "mkdir \"$DATA_SFTP/config/ign-deploy\""
  scap 'mkdir under the data dir (any name): data/zz-c6-datadir' "mkdir \"$DATA_SFTP/zz-c6-datadir\""

  sub 'C6-4 — read core and production'
  scap 'ls -1 of core' "@ls -1 \"$RES_SFTP/core\""
  scap 'ls -1 of core/ignition' "@ls -1 \"$RES_SFTP/core/ignition\""
  scap 'ls -1 of production' "@ls -1 \"$P\""
  mkdir -p "$LOC/rd_core" "$LOC/rd_prod"
  scap 'get -R of one core directory (security-properties)' "get -R \"$RES_SFTP/core/ignition/security-properties\" \"$LOC/rd_core\""
  scap 'get -R of production (whatever it holds)' "get -R \"$P\" \"$LOC/rd_prod\""
  run_block 'what the reads left locally (relative names, sizes; sha256 of the files)' "" bash -c 'cd "$1" && find . -type f -exec wc -c {} \; | sort -k2 && find . -type f -exec shasum -a 256 {} \;' _ "$LOC"
  scap 'ls -1 of the tag definitions in core (tag trees live under core/ignition/tag-definition)' "@ls -1 \"$RES_SFTP/core/ignition/tag-definition\""
  scap 'ls -1 of the other top-level data dir entries the plan does not need (config, data/projects), to bound the read scope' "@ls -1 \"$DATA_SFTP/config\"" "@ls -1 \"$DATA_SFTP/projects\""

  sub 'C6-scan-after — gateway scan state after the probes'
  icap 'GET scan/config (after)' api call --method GET --path /data/api/v1/scan/config

  sub 'C6-cleanup'
  scap 'remove the home staging probe: files' 'rm "ign-deploy/gw/stage2.txt"' 'rm "ign-deploy/gw/abs.txt"'
  scap 'remove the home staging probe: unit directory' 'rm "ign-deploy/gw/unit/f.txt"' 'rmdir "ign-deploy/gw/unit"'
  scap 'remove the home staging probe: directories' 'rmdir "ign-deploy/gw"' 'rmdir "ign-deploy"'
  rcap 'the home after the cleanup' 'dir /b "%USERPROFILE%"'
  scap 'defensive removal: file in production (only present if a rename or put surprisingly succeeded)' "rm \"$P/zz-c6-moved.txt\""
  scap 'defensive removal: file in production (direct put)' "rm \"$P/zz-c6-direct.txt\""
  scap 'defensive removal: directory in production' "rm \"$P/zz-c6-unit/f.txt\"" "rmdir \"$P/zz-c6-unit\""
  scap 'production after all defensive removals' "@ls -1 \"$P\""
  scap 'defensive removal of anything a surprising success left behind (each line may fail with no such file; the batch aborts at the first)' \
    "rmdir \"$RES_SFTP/zz-c6-root-probe\""
  scap 'defensive removal: collection probe' "rmdir \"$RES_SFTP/zz-c6-collection\""
  scap 'defensive removal: data dir probe' "rmdir \"$DATA_SFTP/zz-c6-datadir\""
  scap 'defensive removal: data/ign-deploy' "rmdir \"$DATA_SFTP/ign-deploy\""
  scap 'defensive removal: data/config/ign-deploy' "rmdir \"$DATA_SFTP/config/ign-deploy\""
}

# ===========================================================================
c5ntfs() {
  refuse_if_present 'C5-NTFS —'
  sect 'C5-NTFS — on-disk names the 8.3.6 gateway writes on NTFS for hostile tag folder names (deferred here by 21-CONTEXT D-04)'
  prose "Provider \`CAP22H\` is created on the capture gateway \`$PROFILE\`, nine folders are imported one by one through the gateway's NATIVE route \`POST /data/api/v1/tags/import?provider=CAP22H&type=json&collisionPolicy=Abort\` (via \`ign api call\`; this host has no WebDev module, so \`ign tags import\`, which rides the WebDev routes, cannot be used here, see attempt 1 above), the asynchronous flush is given 20 s, the provider's tag directories are listed over sftp as \`$SSH_USER\` and from the cmd.exe side, the gateway log is read, and the provider is deleted. Render stays fail-closed on Windows-illegal names whatever this shows."
  h_def

  icap 'ign webdev status (is the WebDev module and the ign-cli bundle there?)' webdev status
  icap 'ign tags provider create CAP22H' tags provider create CAP22H

  local long200
  long200=$(printf 'L%.0s' $(seq 1 200))
  local names=("plain" "CON" "aux" "NUL" "trail " "trail." "a:b" "$long200" "lpt1")
  local name json label
  for name in "${names[@]}"; do
    json=$(jq -cn --arg n "$name" '{tags:[{name:$n,tagType:"Folder",tags:[{name:"T1",tagType:"AtomicTag",dataType:"Int4",valueSource:"memory",value:1}]}]}')
    label="native tags/import: folder named '${name:0:24}' (${#name} chars)"
    run_block "$label" "" "$IGN" --profile "$PROFILE" --json api call --method POST --path /data/api/v1/tags/import \
      --query provider=CAP22H --query type=json --query collisionPolicy=Abort \
      --header 'Content-Type: application/octet-stream' --data "$json"
  done

  prose 'Waiting 20 s for the gateway to flush the tag files (the flush is asynchronous, Phase 21 CD4).'
  sleep 20
  local TD="$RES_SFTP/core/ignition/tag-definition"
  scap 'sftp listing of the provider tag directory (ls -1)' "@ls -1 \"$TD/CAP22H\""
  scap 'sftp listing of the provider tag directory (ls -la)' "@ls -la \"$TD/CAP22H\""
  rcap 'cmd.exe side: dir /x (8.3 names) of the provider directory and the folders inside it' "dir /x \"$RES_WIN\\core\\ignition\\tag-definition\\CAP22H\" & dir /b /s \"$RES_WIN\\core\\ignition\\tag-definition\\CAP22H\""
  pscap 'PowerShell side: every entry under the provider directory with its name length and a hex dump of the first 24 characters of the name' "Get-ChildItem -LiteralPath (Join-Path '$RES_WIN' 'core\\ignition\\tag-definition\\CAP22H') -Recurse -Force | ForEach-Object { \$n=\$_.Name; \$h=(\$n.Substring(0,[Math]::Min(24,\$n.Length)).ToCharArray() | ForEach-Object { '{0:X4}' -f [int]\$_ }) -join ' '; ('{0} chars [{1}] {2}' -f \$n.Length,\$h,\$n.Substring(0,[Math]::Min(24,\$n.Length))) }" prelude
  run_block 'gateway log, WARN and above in the last 4 minutes (timestamp, logger, level, message only; stacks and mdc dropped by jq)' "" bash -c '"$1" --profile "$2" --json logs --min-level warn --since 4min --limit 200 | jq -c ".data.items[] | {timestamp, loggerName, level, message}"' _ "$IGN" "$PROFILE"
  icap 'ign tags provider list (what the gateway thinks it holds)' tags provider list
  icap 'ign tags provider delete CAP22H --yes' --yes tags provider delete CAP22H
  sleep 5
  scap 'sftp listing after the provider delete (is the directory gone?)' "@ls -1 \"$TD\""
  rcap 'cmd.exe side after the delete: CAP22H still there?' "dir /b \"$RES_WIN\\core\\ignition\\tag-definition\" | findstr /i CAP22H"
}

# ===========================================================================
case "$SUB" in
  preflight) preflight ;;
  c7a) c7a ;;
  c6) c6 ;;
  c5ntfs) c5ntfs ;;
  clean) SPLICED=1; scratch_clean; cat "$SEC" ;;
esac

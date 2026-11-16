#!/usr/bin/env bash
# capture27-vm.sh STEP [ARG]
#
# Phase 27 (27-07) capture driver for the two TEST gateway VMs: the Linux VM
# (Azure `ign-lnx-836`) and the Windows VM (Azure `ign-win-836`). It records
# G1 (write access, ownership and ACLs of a deploy-account restore, D-22),
# G4 (save, restore and manifest times at production shape, D-18) and G8
# (Windows specifics) as the key-only, non-admin deploy account over ssh.
#
#   project-up    create the disposable project CapApp27 through the gateway REST
#                 API on each VM (refuses when it already exists)
#   upload        one archive of the emitted gateway `.git` and one of the emitted
#                 bare remote per VM, one sftp put each into the account home,
#                 extracted host-locally into ign-live-scratch/git27/src.git and
#                 ign-live-scratch/git27/remote.git
#   g1-before     owner, group, mode and ACL of data/projects and CapApp27, the
#                 gateway service account, and a probe create/remove as the deploy
#                 account (expected: denied)
#   g4            host-local copy (save), copy back (restore) and manifest, three
#                 runs each, median against the 27-06 host budgets; Linux cp -a,
#                 Windows robocopy and Copy-Item
#   g8            Windows only: Hidden attribute after each tool's restore, longest
#                 path, long-path setting, PowerShell edition, antivirus state,
#                 git module presence and version (REST module list)
#   g8-jgit       Windows only: core.* and filter config, index lock and index
#                 header of the git module's own clone CapApp27g
#   g1-after      after the human grant: probe create/remove in the project and in
#                 a `.git` copied there from src.git, owner/mode/ACL of what the
#                 copy created, then the copy is removed
#   project-down  remove CapApp27 only and quote the REST project list
#   scratch-down  remove ign-live-scratch/git27 (NOT run in 27-07: 27-09 and 27-11
#                 run their WinScripts on the uploaded `.git`; 27-25 runs it last)
#   emit NAME     print the remote script NAME (offline test aid), e.g. lnx-g4
#   guard         the guard only: ssh answers with the pinned key, the gateway tunnel
#                 comes up and gateway-info through it names the expected test gateway
#   vm-gate TEST  run the ignored live gate TEST (`^[a-z0-9_]+$`) of the one VM in
#                 CAPTURE27_VMS (`live_transport_lnxvm` or `live_transport_wincap`)
#                 from the repo root: guard, tunnel, the gate's own environment, cargo
#                 inside the redact-and-tee runner; it passes only on cargo exit 0 plus
#                 a `LIVE-RAN TEST` line matched as a whole word at the end of its line
#   winvm-handover  `vm-gate wincap_git_handover` with the Windows VM fixed; it passes
#                 PRODSHAPE_GIT_OUT and IGN_PROD_SHAPE_FILE through when exported
#                 (`lnxvm-handover`, the Linux gate run ON ign-lnx-836, comes in 27-32)
#
# Environment. Addresses, accounts and key paths come only from here (or from
# the user's own 0600 files named below); nothing is written to a tracked file.
#   CAPTURE27_VMS               "lnx win" (default) or one of them
#   CAPTURE27_RUN               the log name prefix (default 27-07)
#   PRODSHAPE_GIT_OUT           `upload` only: the directory the 27-06 generator
#                               emitted (holds capapp.git and data/projects/CapApp/.git)
#   Linux:   IGN_LNXVM_HOST (default: contents of $HOME/.cache/ign-lnxvm01/host)
#            IGN_LNXVM_USER (svc-ignition-deploy), IGN_LNXVM_KEY_FILE
#            (default $HOME/.cache/ign-lnxvm01/svc-ignition-deploy_ed25519),
#            IGN_LNXVM_KNOWN_HOSTS (default $HOME/.cache/ign-lnxvm01/known_hosts),
#            IGN_LNXVM_CONFIG (the ign config holding the profile; default
#            $HOME/.cache/ign-lnxvm01/ign-config.toml), IGN_LNXVM_PROFILE
#            (azure-lnx), IGN_LNXVM_DATA_DIR (/usr/local/bin/ignition/data),
#            IGN_LNXVM_GATEWAY_NAME (Ignition-ign-lnx-836), IGN_LNXVM_GATEWAY_PORT (8088)
#   Windows: IGN_WINVM_HOST (REQUIRED: the address changes on every VM start; read
#            it after the start and export it), IGN_WINVM_KEY (key text) or
#            IGN_WINVM_KEY_FILE, IGN_WINVM_PROFILE, IGN_WINVM_KNOWN_HOSTS (default:
#            the first of $HOME/.cache/ign-wflwvm01/known_hosts_836 and known_hosts
#            that has an entry for the host), IGN_WINVM_USER (svc-ignition-deploy),
#            IGN_WINVM_DATA_DIR (C:\Program Files\Inductive Automation\Ignition\data),
#            IGN_WINVM_GATEWAY_NAME (Ignition-ignwin836), IGN_WINVM_GATEWAY_PORT (8088)
#   CAPTURE27_WIN_TOOL          robocopy (default) or copyitem: the Windows copy tool
#                               g1-after uses (the G4 verdict names it)
#   IGN_BIN                     the ign binary (default ign)
#
# Safety. The pinned known_hosts file must exist and carry an entry for the host
# BEFORE the network is touched; ssh runs with StrictHostKeyChecking=yes. Every
# step first (1) answers over ssh with the pinned key, (2) opens a pinned
# `ssh -L` tunnel from a free port on 127.0.0.1 to the gateway port on the VM's
# own loopback, exports IGNITION_URL for the run as http://127.0.0.1:<port> and
# (3) refuses (exit 2) unless gateway-info through the tunnel names the expected
# test gateway. The gateway serves plain HTTP, so its API token now stays on the
# loopback and inside ssh: nothing is sent to the gateway without the tunnel, and
# there is no fallback to the VM's address (a forward the VM's sshd refuses, or one
# that never answers, stops the step). The tunnel is closed when the step ends.
# Run this only against the throwaway test VMs, never a production host.
# Output is passed through a filter that replaces the host addresses before it
# reaches the terminal or the log. Never enable `set -x` here.
#
# Each step tees its output to $HOME/.cache/ign-git27-vm/evidence/<run>-<step>.log
# (vm-gate: <run>-<step>-<test>.log) and prints `LIVE-RAN capture27-vm <step>` as
# its last line only when every selected VM completed it.
# shellcheck disable=SC2329,SC2034,SC2016
# (the R_*, B_* and PARAMS_* names are reached indirectly by emit_script and
# win_script; the PowerShell text is deliberately single-quoted)
set -euo pipefail
export LC_ALL=C

STEP=${1:-}
case "$STEP" in
  project-up | upload | g1-before | g4 | g8 | g8-jgit | g1-after | project-down | scratch-down | emit | guard | vm-gate | winvm-handover) ;;
  *)
    echo "usage: capture27-vm.sh project-up|upload|g1-before|g4|g8|g8-jgit|g1-after|project-down|scratch-down|guard|winvm-handover|emit NAME|vm-gate TEST" >&2
    exit 64
    ;;
esac
GATE_TEST=
LOG_STEP=
case "$STEP" in
  vm-gate) GATE_TEST=${2:-} ;;
  winvm-handover)
    GATE_TEST=wincap_git_handover
    STEP=vm-gate
    CAPTURE27_VMS=win
    LOG_STEP=winvm-handover
    ;;
esac
if [ "$STEP" = vm-gate ]; then
  case "$GATE_TEST" in
    '' | *[!a-z0-9_]*)
      echo "REFUSING: vm-gate needs a test name matching ^[a-z0-9_]+\$" >&2
      exit 64
      ;;
  esac
fi

PROJ=CapApp27
PROJG=${CAPTURE27_PROJG:-CapApp27g}
SCRATCH_REL=ign-live-scratch/git27
IGN=${IGN_BIN:-ign}
VMS=${CAPTURE27_VMS:-lnx win}
RUN=${CAPTURE27_RUN:-27-07}
case "$RUN" in
  '' | *[!A-Za-z0-9._-]*)
    echo "REFUSING: CAPTURE27_RUN holds characters outside [A-Za-z0-9._-]" >&2
    exit 64
    ;;
esac
REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)
CACHE=$HOME/.cache/ign-git27-vm
EVID=$CACHE/evidence

# 27-06 host budgets, seconds (HOST_LINUX_SAVE, HOST_LINUX_RESTORE_VERIFY,
# HOST_WINDOWS_SAVE, HOST_WINDOWS_RESTORE_VERIFY).
BUDGET_LNX_SAVE=60
BUDGET_LNX_RV=60
BUDGET_WIN_SAVE=120
BUDGET_WIN_RV=120

die() {
  echo "REFUSING: $*" >&2
  exit 2
}

# ---------------------------------------------------------------------------
# Remote scripts. Linux scripts are bash text fed to `bash -s` over ssh with the
# parameters exported first; Windows scripts are PowerShell files put into the
# account home by sftp and run with -File (a stdin or -EncodedCommand script
# would hit the command-length limit).
# ---------------------------------------------------------------------------

R_lnx_g1_before() {
  cat <<'EOS'
set -u
proj_dir="$DATA/projects/$PROJ"
echo "== identity"; id
echo "== umask"; umask
echo "== owner, group, mode"
stat -c 'owner=%U group=%G mode=%a %n' "$DATA" "$DATA/projects" "$proj_dir" 2>&1
echo "== processes named ignition (user, command)"
ps -eo user:24,comm 2>/dev/null | awk 'tolower($2) ~ /ignition/ {print $1, $2}' | sort -u
for d in "$DATA/projects" "$proj_dir"; do
  echo "== getfacl $d"
  if command -v getfacl >/dev/null 2>&1; then getfacl -p "$d" 2>&1; else echo "getfacl: not installed"; fi
done
echo "== listing of the project directory"
ls -la "$proj_dir" 2>&1
echo "== probe file in the project directory"
err=$(mktemp)
if { : > "$proj_dir/.ign-probe-27"; } 2> "$err"; then
  rm -f "$proj_dir/.ign-probe-27"
  echo "probe-file: created and removed"
else
  echo "probe-file: denied: $(cat "$err")"
fi
echo "== probe .git directory in the project directory"
if mkdir "$proj_dir/.git" 2> "$err"; then
  rmdir "$proj_dir/.git"
  echo "probe-git-dir: created and removed"
else
  echo "probe-git-dir: denied: $(cat "$err")"
fi
rm -f "$err"
EOS
}

R_lnx_g4() {
  cat <<'EOS'
set -u
cd "$HOME/$SCRATCH_REL" || { echo "no scratch directory: run upload first" >&2; exit 2; }
[ -d src.git ] || { echo "no src.git: run upload first" >&2; exit 2; }
now() { date +%s.%N; }
elapsed() { awk -v a="$1" -v b="$2" 'BEGIN { printf "%.2f", b - a }'; }
median3() { printf '%s\n' "$@" | sort -n | sed -n 2p; }
manifest() {
  # sorted (path, size) for objects/**, sha256 of every other file, then one digest
  (
    cd "$1" || exit 1
    {
      find objects -type f -printf 'O %p %s\n'
      find . -type f ! -path './objects/*' -print0 | sort -z | xargs -0 -r sha256sum | sed 's/^/F /'
    } | LC_ALL=C sort | sha256sum | cut -d' ' -f1
  )
}
echo "== source: $(find src.git -type f | wc -l | tr -d ' ') files, $(du -sb src.git | cut -f1) bytes"
m0=$(now); src_digest=$(manifest src.git 2>/dev/null); m1=$(now)
echo "G4-MANIFEST-SOURCE seconds=$(elapsed "$m0" "$m1")"
s1=""; s2=""; s3=""; r1=""; r2=""; r3=""
for i in 1 2 3; do
  rm -rf g4-save g4-restore
  mkdir g4-save g4-restore
  t0=$(now); cp -a src.git g4-save/.git; t1=$(now)
  save=$(elapsed "$t0" "$t1")
  t2=$(now); cp -a g4-save/.git g4-restore/.git; t3=$(now)
  restore=$(elapsed "$t2" "$t3")
  t4=$(now); rest_digest=$(manifest g4-restore/.git 2>/dev/null); t5=$(now)
  verify=$(elapsed "$t4" "$t5")
  rv=$(elapsed "$t2" "$t5")
  if [ "$rest_digest" = "$src_digest" ]; then eq=equal; else eq=differs; fi
  echo "G4-RUN lnx cp-a run=$i save=$save restore=$restore manifest=$verify restore_verify=$rv manifest_vs_source=$eq"
  case $i in 1) s1=$save; r1=$rv ;; 2) s2=$save; r2=$rv ;; 3) s3=$save; r3=$rv ;; esac
done
ms=$(median3 "$s1" "$s2" "$s3"); mr=$(median3 "$r1" "$r2" "$r3")
within_s=$(awk -v m="$ms" -v b="$BUDGET_SAVE" 'BEGIN { print (m <= b) ? "yes" : "NO" }')
within_r=$(awk -v m="$mr" -v b="$BUDGET_RV" 'BEGIN { print (m <= b) ? "yes" : "NO" }')
echo "G4-MEDIAN lnx cp-a save=$ms budget=$BUDGET_SAVE within=$within_s"
echo "G4-MEDIAN lnx cp-a restore_verify=$mr budget=$BUDGET_RV within=$within_r"
rm -rf g4-save g4-restore
EOS
}

R_lnx_g1_after() {
  cat <<'EOS'
set -u
proj_dir="$DATA/projects/$PROJ"
gitdir="$proj_dir/.git"
src="$HOME/$SCRATCH_REL/src.git"
[ -d "$src" ] || { echo "no src.git: run upload first" >&2; exit 2; }
err=$(mktemp)
probe() { # DIR LABEL
  if { : > "$1/.ign-probe-27"; } 2> "$err"; then rm -f "$1/.ign-probe-27"; echo "probe-file $2: created and removed"; else echo "probe-file $2: denied: $(cat "$err")"; fi
}
show() { # PATH
  stat -c 'owner=%U group=%G mode=%a %n' "$1" 2>&1
  if command -v getfacl >/dev/null 2>&1; then getfacl -p "$1" 2>&1 | grep -v '^# file:' ; fi
}
echo "== identity"; id
echo "== project directory after the grant"
show "$proj_dir"
probe "$proj_dir" "project-dir"
for variant in cp-a cp-R-no-preserve-mode; do
  echo "######## variant $variant"
  rm -rf "$gitdir"
  case "$variant" in
    cp-a) cp -a "$src" "$gitdir" ;;
    cp-R-no-preserve-mode) cp -R --no-preserve=mode,ownership "$src" "$gitdir" ;;
  esac
  echo "copied: $(find "$gitdir" -type f | wc -l | tr -d ' ') files"
  probe "$gitdir" "inside-.git"
  probe "$gitdir/objects" "inside-.git/objects"
  pack=$(find "$gitdir/objects/pack" -type f -name '*.pack' 2>/dev/null | head -n1)
  fan=$(find "$gitdir/objects" -mindepth 1 -maxdepth 1 -type d -name '[0-9a-f][0-9a-f]' 2>/dev/null | head -n1)
  for p in "$gitdir" "$gitdir/index" "$gitdir/HEAD" "$gitdir/refs" "$gitdir/objects" "$gitdir/objects/pack" ${pack:+"$pack"} ${fan:+"$fan"}; do
    echo "-- $(echo "$p" | sed "s|^$DATA|<data>|")"
    show "$p"
  done
done
echo "== removing the copy as the deploy account"
rm -rf "$gitdir"
if [ -e "$gitdir" ]; then echo "removed: NO"; else echo "removed: yes"; fi
ls -la "$proj_dir" 2>&1
rm -f "$err"
EOS
}

R_lnx_upload_extract() {
  cat <<'EOS'
set -eu
cd "$HOME"
dest="$SCRATCH_REL/$NAME"
rm -rf "$dest"
mkdir -p "$dest"
t0=$(date +%s.%N)
tar -xf "ign-git27-$NAME.tar" -C "$dest"
t1=$(date +%s.%N)
rm -f "ign-git27-$NAME.tar"
echo "extracted $NAME: $(find "$dest" -type f | wc -l | tr -d ' ') files, $(du -sb "$dest" | cut -f1) bytes, $(awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.2f", b - a }') s"
EOS
}

R_lnx_scratch_down() {
  cat <<'EOS'
set -eu
case "$HOME/$SCRATCH_REL" in
  "$HOME"/ign-live-scratch/git27) ;;
  *) echo "refusing to remove an unexpected path" >&2; exit 2 ;;
esac
rm -rf "${HOME:?}/${SCRATCH_REL:?}"
rm -f "$HOME"/ign-git27-*.tar
if [ -e "$HOME/$SCRATCH_REL" ]; then echo "scratch-down: still present"; exit 1; else echo "scratch-down: removed"; fi
EOS
}

R_lnx_project_dir() {
  cat <<'EOS'
set -u
for _i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
  if [ -d "$DATA/projects/$PROJ" ]; then
    stat -c 'owner=%U group=%G mode=%a %n' "$DATA/projects/$PROJ" | sed "s|$DATA|<data>|"
    exit 0
  fi
  sleep 2
done
echo "project directory did not appear"
exit 1
EOS
}

R_lnx_project_gone() {
  cat <<'EOS'
set -u
for _i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
  if [ ! -e "$DATA/projects/$PROJ" ]; then echo "project directory: absent"; exit 0; fi
  sleep 2
done
echo "project directory: still present"
ls -la "$DATA/projects/$PROJ" 2>&1
exit 1
EOS
}

R_win_common() {
  cat <<'EOS'
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Scratch = Join-Path $env:USERPROFILE 'ign-live-scratch\git27'
function Show-Acl([string]$p) {
  Write-Output "-- acl $p"
  try {
    $a = Get-Acl -LiteralPath $p
    Write-Output ("owner=" + $a.Owner + " inheritanceBlocked=" + $a.AreAccessRulesProtected)
    foreach ($r in $a.Access) {
      Write-Output ("  {0} | {1} | {2} | inherited={3} | flags={4}" -f $r.IdentityReference, $r.AccessControlType, $r.FileSystemRights, $r.IsInherited, $r.InheritanceFlags)
    }
  } catch { Write-Output ("acl-failed: " + $_.Exception.Message) }
}
function Test-Probe([string]$dir, [string]$label) {
  $probe = Join-Path $dir '.ign-probe-27'
  try {
    New-Item -ItemType File -Path $probe -ErrorAction Stop | Out-Null
    Remove-Item -LiteralPath $probe -Force -ErrorAction Stop
    Write-Output "probe-file ${label}: created and removed"
  } catch { Write-Output ("probe-file ${label}: denied: " + $_.Exception.Message) }
}
function Remove-Tree([string]$p) {
  if (-not $p.StartsWith($env:USERPROFILE + '\ign-live-scratch\') -and -not $p.Contains('\projects\CapApp27\')) { throw "refusing to remove $p" }
  if (Test-Path -LiteralPath $p) { & cmd.exe /c rmdir /s /q $p | Out-Null }
}
function Get-Manifest([string]$Dir) {
  $root = (Resolve-Path -LiteralPath $Dir).ProviderPath.TrimEnd('\')
  $lines = New-Object System.Collections.Generic.List[string]
  $sha = [System.Security.Cryptography.SHA256]::Create()
  foreach ($f in Get-ChildItem -LiteralPath $root -Recurse -Force -File) {
    $rel = $f.FullName.Substring($root.Length + 1).Replace('\', '/')
    if ($rel.StartsWith('objects/')) { $lines.Add("O $rel $($f.Length)") }
    else {
      $h = [BitConverter]::ToString($sha.ComputeHash([IO.File]::ReadAllBytes($f.FullName))).Replace('-', '').ToLower()
      $lines.Add("F $rel $h")
    }
  }
  $arr = $lines.ToArray()
  [Array]::Sort($arr, [StringComparer]::Ordinal)
  $d = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes([string]::Join("`n", $arr)))).Replace('-', '').ToLower()
  return $d
}
function Copy-Robo([string]$src, [string]$dst) {
  & robocopy.exe $src $dst /E /COPY:DAT /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "robocopy exit $LASTEXITCODE" }
}
function Copy-Ps([string]$src, [string]$dst) {
  Copy-Item -LiteralPath $src -Destination $dst -Recurse -Force
}
function Copy-With([string]$tool, [string]$src, [string]$dst) {
  if ($tool -eq 'robocopy') { Copy-Robo $src $dst } else { Copy-Ps $src $dst }
}
function Show-GwService {
  # A non-admin deploy account cannot query Win32_Service (CIM "Access denied") and
  # sees no services; that must not abort the step (g1-after would leave its copy behind).
  try {
    $s = @(Get-CimInstance Win32_Service -ErrorAction Stop | Where-Object { $_.Name -like '*gnition*' -or $_.DisplayName -like '*gnition*' })
    foreach ($x in $s) { Write-Output ("service={0} startname={1} state={2}" -f $x.Name, $x.StartName, $x.State) }
    if ($s.Count -eq 0) { Write-Output "service: none named *gnition* visible to this account" }
  } catch { Write-Output ("service-accounts: not readable as this account (" + $_.Exception.Message.Trim() + ")") }
  foreach ($p in @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -match 'java|ignition' })) { Write-Output ("process={0} id={1}" -f $p.ProcessName, $p.Id) }
}
function Get-Median3($a) { return (($a | Sort-Object)[1]) }
function Fmt([double]$x) { return $x.ToString('0.00', [Globalization.CultureInfo]::InvariantCulture) }
EOS
}

# PowerShell requires `param` first; each Windows script is built as
# param block + common + body (see win_script below).
B_win_g1_before() {
  cat <<'EOS'
$projDir = Join-Path $Data "projects\$Proj"
Write-Output "== identity"; Write-Output (& whoami.exe)
Write-Output "== gateway service accounts (name, log-on account, state)"
Show-GwService
Write-Output "== owner and ACL"
Show-Acl (Join-Path $Data 'projects')
Show-Acl $projDir
Write-Output "== listing of the project directory"
Get-ChildItem -LiteralPath $projDir -Force | ForEach-Object { Write-Output ("{0} {1}" -f $_.Mode, $_.Name) }
Write-Output "== probe file in the project directory"
Test-Probe $projDir 'project-dir'
Write-Output "== probe .git directory in the project directory"
$g = Join-Path $projDir '.git'
try {
  New-Item -ItemType Directory -Path $g -ErrorAction Stop | Out-Null
  Remove-Item -LiteralPath $g -Force -ErrorAction Stop
  Write-Output "probe-git-dir: created and removed"
} catch { Write-Output ("probe-git-dir: denied: " + $_.Exception.Message) }
EOS
}

PARAMS_win_g1_before='param([string]$Data, [string]$Proj)'

B_win_g4() {
  cat <<'EOS'
$src = Join-Path $Scratch 'src.git'
if (-not (Test-Path -LiteralPath $src)) { Write-Output 'no src.git: run upload first'; exit 2 }
$stat = Get-ChildItem -LiteralPath $src -Recurse -Force -File | Measure-Object Length -Sum
Write-Output ("== source: {0} files, {1} bytes" -f $stat.Count, $stat.Sum)
$sw = [Diagnostics.Stopwatch]::StartNew(); $srcDigest = Get-Manifest $src; $sw.Stop()
Write-Output ("G4-MANIFEST-SOURCE seconds={0}" -f (Fmt $sw.Elapsed.TotalSeconds))
foreach ($tool in @('robocopy', 'copyitem')) {
  $saves = @(); $rvs = @(); $allEqual = $true
  for ($i = 1; $i -le 3; $i++) {
    $sv = Join-Path $Scratch 'g4-save'; $rs = Join-Path $Scratch 'g4-restore'
    Remove-Tree $sv; Remove-Tree $rs
    New-Item -ItemType Directory -Path $sv, $rs | Out-Null
    $sw = [Diagnostics.Stopwatch]::StartNew(); Copy-With $tool $src (Join-Path $sv '.git'); $sw.Stop(); $save = $sw.Elapsed.TotalSeconds
    $sw = [Diagnostics.Stopwatch]::StartNew(); Copy-With $tool (Join-Path $sv '.git') (Join-Path $rs '.git'); $sw.Stop(); $restore = $sw.Elapsed.TotalSeconds
    $sw = [Diagnostics.Stopwatch]::StartNew(); $d = Get-Manifest (Join-Path $rs '.git'); $sw.Stop(); $verify = $sw.Elapsed.TotalSeconds
    $eq = if ($d -eq $srcDigest) { 'equal' } else { 'differs'; $allEqual = $false }
    Write-Output ("G4-RUN win {0} run={1} save={2} restore={3} manifest={4} restore_verify={5} manifest_vs_source={6}" -f $tool, $i, (Fmt $save), (Fmt $restore), (Fmt $verify), (Fmt ($restore + $verify)), $eq)
    $saves += $save; $rvs += ($restore + $verify)
  }
  $ms = Get-Median3 $saves; $mr = Get-Median3 $rvs
  $ws = if ($ms -le $BudgetSave) { 'yes' } else { 'NO' }; $wr = if ($mr -le $BudgetRv) { 'yes' } else { 'NO' }
  Write-Output ("G4-MEDIAN win {0} save={1} budget={2} within={3}" -f $tool, (Fmt $ms), $BudgetSave, $ws)
  Write-Output ("G4-MEDIAN win {0} restore_verify={1} budget={2} within={3}" -f $tool, (Fmt $mr), $BudgetRv, $wr)
  Write-Output ("G4-TOOL-MATCH win {0} manifest={1}" -f $tool, $(if ($allEqual) { 'equal' } else { 'differs' }))
}
Remove-Tree (Join-Path $Scratch 'g4-save'); Remove-Tree (Join-Path $Scratch 'g4-restore')
EOS
}
PARAMS_win_g4='param([double]$BudgetSave, [double]$BudgetRv)'

B_win_g8() {
  cat <<'EOS'
$src = Join-Path $Scratch 'src.git'
if (-not (Test-Path -LiteralPath $src)) { Write-Output 'no src.git: run upload first'; exit 2 }
Write-Output "== PowerShell of this ssh session"
Write-Output ("PSEdition=" + $PSVersionTable.PSEdition + " PSVersion=" + $PSVersionTable.PSVersion.ToString() + " CLR=" + [Environment]::Version.ToString())
Write-Output ("pwsh on PATH: " + [bool](Get-Command pwsh -ErrorAction SilentlyContinue))
foreach ($c in @('robocopy.exe', 'tar.exe')) {
  $cmd = Get-Command $c -ErrorAction SilentlyContinue
  if ($cmd) { Write-Output ("{0}: {1}" -f $c, $cmd.Version) } else { Write-Output ("{0}: not found" -f $c) }
}
Write-Output "== antivirus"
try {
  $mp = Get-MpComputerStatus -ErrorAction Stop
  Write-Output ("AntivirusEnabled={0} RealTimeProtectionEnabled={1}" -f $mp.AntivirusEnabled, $mp.RealTimeProtectionEnabled)
} catch { Write-Output ("Get-MpComputerStatus: " + $_.Exception.Message) }
Write-Output "== long paths"
$lp = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -ErrorAction SilentlyContinue).LongPathsEnabled
Write-Output ("LongPathsEnabled=" + $(if ($null -eq $lp) { 'unset' } else { $lp }))
$srcRoot = (Resolve-Path -LiteralPath $src).ProviderPath.TrimEnd('\')
$maxRel = 0; $maxTop = ''
foreach ($i in Get-ChildItem -LiteralPath $srcRoot -Recurse -Force) {
  $rel = $i.FullName.Substring($srcRoot.Length + 1)
  if ($rel.Length -gt $maxRel) { $maxRel = $rel.Length; $maxTop = $rel.Split('\')[0] }
}
$prefix = (Join-Path $Data "projects\$Proj\.git").Length
Write-Output ("longest path inside src.git: {0} chars (under {1}); under the real project: {2} chars; MAX_PATH 260 exceeded: {3}" -f $maxRel, $maxTop, ($prefix + 1 + $maxRel), (($prefix + 1 + $maxRel) -ge 260))
Write-Output "== attributes after a restore, from a Hidden saved .git (the module's .git is Hidden, Attributes=18)"
$sv = Join-Path $Scratch 'g8-saved'; Remove-Tree $sv; New-Item -ItemType Directory -Path $sv | Out-Null
Copy-Robo $src (Join-Path $sv '.git')
$savedGit = Get-Item -LiteralPath (Join-Path $sv '.git') -Force
$savedGit.Attributes = $savedGit.Attributes -bor [IO.FileAttributes]::Hidden
Write-Output ("saved .git attributes: {0} ({1})" -f [int]$savedGit.Attributes, $savedGit.Attributes)
foreach ($tool in @('robocopy', 'copyitem')) {
  $rs = Join-Path $Scratch ('g8-' + $tool); Remove-Tree $rs; New-Item -ItemType Directory -Path $rs | Out-Null
  Copy-With $tool (Join-Path $sv '.git') (Join-Path $rs '.git')
  $it = Get-Item -LiteralPath (Join-Path $rs '.git') -Force
  Write-Output ("restored by {0}: attributes {1} ({2}); Hidden set: {3}" -f $tool, [int]$it.Attributes, $it.Attributes, (($it.Attributes -band [IO.FileAttributes]::Hidden) -ne 0))
  & attrib.exe +h (Join-Path $rs '.git') | Out-Null
  $it = Get-Item -LiteralPath (Join-Path $rs '.git') -Force
  Write-Output ("restored by {0}: after attrib +h: {1} ({2})" -f $tool, [int]$it.Attributes, $it.Attributes)
  Remove-Tree $rs
}
Remove-Tree $sv
EOS
}
PARAMS_win_g8='param([string]$Data, [string]$Proj)'

B_win_g8_jgit() {
  cat <<'EOS'
$gdir = Join-Path $Data "projects\$ProjG\.git"
if (-not (Test-Path -LiteralPath $gdir -ErrorAction SilentlyContinue)) { Write-Output ("no module clone at projects\{0}\.git" -f $ProjG); exit 2 }
$it = Get-Item -LiteralPath $gdir -Force
Write-Output ("module .git attributes: {0} ({1})" -f [int]$it.Attributes, $it.Attributes)
Write-Output "== config lines that matter (section headers and core, filter, eol keys)"
$cfg = Join-Path $gdir 'config'
Get-Content -LiteralPath $cfg | ForEach-Object { $_.Trim() } |
  Where-Object { $_ -match '^\[' -or $_ -match '^(autocrlf|filemode|symlinks|ignorecase|safecrlf|eol|bare|logallrefupdates|clean|smudge|process|required|sslverify)\b' } |
  ForEach-Object { Write-Output ("  " + $_) }
Write-Output "== owner and ACL of the module's own .git and index"
Show-Acl $gdir
Show-Acl (Join-Path $gdir 'index')
Write-Output "== index header"
$idx = Join-Path $gdir 'index'
try {
  $fs = [IO.File]::Open($idx, 'Open', 'Read', 'ReadWrite')
  $b = New-Object byte[] 12; [void]$fs.Read($b, 0, 12); $fs.Dispose()
  Write-Output ("magic={0} version={1} entries={2}" -f [Text.Encoding]::ASCII.GetString($b, 0, 4), (($b[4] * 16777216) + ($b[5] * 65536) + ($b[6] * 256) + $b[7]), (($b[8] * 16777216) + ($b[9] * 65536) + ($b[10] * 256) + $b[11]))
} catch { Write-Output ("index-read-failed: " + $_.Exception.Message) }
Write-Output "== can .git/index be opened with FileShare.None (no other handle open) while the gateway runs"
try {
  $fs = [IO.File]::Open($idx, 'Open', 'Read', 'None'); $fs.Dispose()
  Write-Output "index-exclusive-open: succeeded (no other process holds the index open)"
} catch { Write-Output ("index-exclusive-open: failed: " + $_.Exception.Message) }
EOS
}
PARAMS_win_g8_jgit='param([string]$Data, [string]$ProjG)'

B_win_g1_after() {
  cat <<'EOS'
$projDir = Join-Path $Data "projects\$Proj"
$gitdir = Join-Path $projDir '.git'
$src = Join-Path $Scratch 'src.git'
if (-not (Test-Path -LiteralPath $src)) { Write-Output 'no src.git: run upload first'; exit 2 }
Write-Output "== identity"; Write-Output (& whoami.exe)
Write-Output "== project directory after the grant"
Show-Acl $projDir
Test-Probe $projDir 'project-dir'
Write-Output ("== copying src.git into the project as .git with {0}" -f $Tool)
Remove-Tree $gitdir
Copy-With $Tool $src $gitdir
$n = (Get-ChildItem -LiteralPath $gitdir -Recurse -Force -File | Measure-Object).Count
Write-Output ("copied: {0} files" -f $n)
$it = Get-Item -LiteralPath $gitdir -Force
Write-Output ("copied .git attributes: {0} ({1})" -f [int]$it.Attributes, $it.Attributes)
Test-Probe $gitdir 'inside-.git'
Test-Probe (Join-Path $gitdir 'objects') 'inside-.git\objects'
$pack = Get-ChildItem -LiteralPath (Join-Path $gitdir 'objects\pack') -Filter '*.pack' -File -ErrorAction SilentlyContinue | Select-Object -First 1
Show-Acl $gitdir
Show-Acl (Join-Path $gitdir 'index')
Show-Acl (Join-Path $gitdir 'HEAD')
Show-Acl (Join-Path $gitdir 'objects')
if ($pack) { Show-Acl $pack.FullName }
Write-Output "== gateway service accounts (the rows above must carry an inherited Modify or FullControl entry for one of them, or a group it is in)"
Show-GwService
Write-Output "== removing the copy as the deploy account"
Remove-Tree $gitdir
Write-Output ("removed: " + (-not (Test-Path -LiteralPath $gitdir)))
EOS
}
PARAMS_win_g1_after='param([string]$Data, [string]$Proj, [string]$Tool)'

B_win_upload_extract() {
  cat <<'EOS'
$dest = Join-Path $Scratch $Name
Remove-Tree $dest
New-Item -ItemType Directory -Path $dest -Force | Out-Null
$tarf = Join-Path $env:USERPROFILE ("ign-git27-" + $Name + ".tar")
$sw = [Diagnostics.Stopwatch]::StartNew()
& tar.exe -xf $tarf -C $dest
if ($LASTEXITCODE -ne 0) { throw "tar exit $LASTEXITCODE" }
$sw.Stop()
Remove-Item -LiteralPath $tarf -Force
$st = Get-ChildItem -LiteralPath $dest -Recurse -Force -File | Measure-Object Length -Sum
Write-Output ("extracted {0}: {1} files, {2} bytes, {3} s" -f $Name, $st.Count, $st.Sum, (Fmt $sw.Elapsed.TotalSeconds))
EOS
}
PARAMS_win_upload_extract='param([string]$Name)'

B_win_scratch_down() {
  cat <<'EOS'
Remove-Tree $Scratch
Get-ChildItem -LiteralPath $env:USERPROFILE -Filter 'ign-git27-*.tar' -File -ErrorAction SilentlyContinue | Remove-Item -Force
Write-Output ("scratch-down: " + $(if (Test-Path -LiteralPath $Scratch) { 'still present' } else { 'removed' }))
EOS
}
PARAMS_win_scratch_down='param()'

B_win_project_state() {
  cat <<'EOS'
$d = Join-Path $Data "projects\$Proj"
for ($i = 0; $i -lt 15; $i++) {
  $present = Test-Path -LiteralPath $d
  if (($Expect -eq 'present' -and $present) -or ($Expect -eq 'absent' -and -not $present)) { break }
  Start-Sleep -Seconds 2
}
if ($present) {
  $a = Get-Acl -LiteralPath $d
  Write-Output ("project directory: present, owner=" + $a.Owner)
} else { Write-Output "project directory: absent" }
if ($Expect -eq 'present' -and -not $present) { exit 1 }
if ($Expect -eq 'absent' -and $present) { exit 1 }
EOS
}
PARAMS_win_project_state='param([string]$Data, [string]$Proj, [string]$Expect)'

# win_script NAME: param block first (PowerShell requires it), then the helpers,
# then the body.
win_script() {
  local name=$1 var
  var=PARAMS_win_${name//-/_}
  printf '%s\n' "${!var}"
  R_win_common
  "B_win_${name//-/_}"
}

# emit NAME: lnx-NAME or win-NAME script text.
emit_script() {
  local n=$1
  case "$n" in
    lnx-*) n=${n#lnx-}; "R_lnx_${n//-/_}" ;;
    win-*) win_script "${n#win-}" ;;
    *) die "emit: name must start with lnx- or win-" ;;
  esac
}

if [ "$STEP" = emit ]; then
  emit_script "${2:?emit NAME}"
  exit 0
fi

# ---------------------------------------------------------------------------
# Everything below touches the network.
# ---------------------------------------------------------------------------

TMPD=$(mktemp -d)
chmod 700 "$TMPD"
TUN_PIDFILE=$TMPD/tunnel.pid
TUN_PORT=
# The tunnel's pid lives in a file: the step runs in a subshell, and an EXIT trap is not
# inherited by it, so this trap (the main shell's) is what closes a tunnel left open.
cleanup() {
  tunnel_down
  /bin/rm -rf "$TMPD"
}
trap cleanup EXIT
trap 'exit 130' INT HUP
trap 'exit 143' TERM

command -v jq >/dev/null 2>&1 || die "jq is required"
command -v perl >/dev/null 2>&1 || die "perl is required"
mkdir -p "$EVID"
chmod 700 "$CACHE" "$EVID"

# vm_select lnx|win: sets the per-VM globals and checks the pin before any network call.
vm_select() {
  VM=$1
  case "$VM" in
    lnx)
      HOST=${IGN_LNXVM_HOST:-}
      if [ -z "$HOST" ] && [ -r "$HOME/.cache/ign-lnxvm01/host" ]; then HOST=$(tr -d '[:space:]' < "$HOME/.cache/ign-lnxvm01/host"); fi
      SSH_USER=${IGN_LNXVM_USER:-svc-ignition-deploy}
      KEYFILE=${IGN_LNXVM_KEY_FILE:-$HOME/.cache/ign-lnxvm01/svc-ignition-deploy_ed25519}
      PIN=${IGN_LNXVM_KNOWN_HOSTS:-$HOME/.cache/ign-lnxvm01/known_hosts}
      PROFILE=${IGN_LNXVM_PROFILE:-azure-lnx}
      IGN_CFG=${IGN_LNXVM_CONFIG:-$HOME/.cache/ign-lnxvm01/ign-config.toml}
      DATA=${IGN_LNXVM_DATA_DIR:-/usr/local/bin/ignition/data}
      GW_NAME=${IGN_LNXVM_GATEWAY_NAME:-Ignition-ign-lnx-836}
      GW_PORT=${IGN_LNXVM_GATEWAY_PORT:-8088}
      # the profile reads its token from IGNITION_TOKEN_AZURE_LNX; default it from the user's cache file
      if [ -z "${IGNITION_TOKEN_AZURE_LNX:-}" ] && [ -r "$HOME/.cache/ign-lnxvm01/api-token" ]; then
        IGNITION_TOKEN_AZURE_LNX=$(tr -d '[:space:]' < "$HOME/.cache/ign-lnxvm01/api-token")
        export IGNITION_TOKEN_AZURE_LNX
      fi
      BUDGET_SAVE=$BUDGET_LNX_SAVE
      BUDGET_RV=$BUDGET_LNX_RV
      ;;
    win)
      HOST=${IGN_WINVM_HOST:-}
      SSH_USER=${IGN_WINVM_USER:-svc-ignition-deploy}
      KEYFILE=${IGN_WINVM_KEY_FILE:-}
      PIN=${IGN_WINVM_KNOWN_HOSTS:-}
      PROFILE=${IGN_WINVM_PROFILE:-}
      IGN_CFG=
      DATA=${IGN_WINVM_DATA_DIR:-C:\\Program Files\\Inductive Automation\\Ignition\\data}
      GW_NAME=${IGN_WINVM_GATEWAY_NAME:-Ignition-ignwin836}
      GW_PORT=${IGN_WINVM_GATEWAY_PORT:-8088}
      BUDGET_SAVE=$BUDGET_WIN_SAVE
      BUDGET_RV=$BUDGET_WIN_RV
      ;;
    *) die "unknown VM '$VM'" ;;
  esac
  [ -n "$HOST" ] || die "no host for $VM (Windows: export IGN_WINVM_HOST after the VM start; the address changes on every start)"
  case "$HOST$SSH_USER" in
    -* | *[[:space:]]* | *[[:cntrl:]]*) die "host or user is malformed" ;;
  esac
  [ -n "$PROFILE" ] || die "no ign profile for $VM"
  if [ "$VM" = win ]; then
    if [ -z "$PIN" ]; then
      local cand
      for cand in "$HOME/.cache/ign-wflwvm01/known_hosts_836" "$HOME/.cache/ign-wflwvm01/known_hosts"; do
        if [ -s "$cand" ] && grep -q -F "$HOST " "$cand"; then PIN=$cand; break; fi
      done
      [ -n "$PIN" ] || die "no pinned known_hosts file has an entry for the Windows VM's current address; pin the host key after its fingerprint equals the one on the VM console, never StrictHostKeyChecking=no"
    fi
    if [ -z "$KEYFILE" ]; then
      [ -n "${IGN_WINVM_KEY:-}" ] || die "IGN_WINVM_KEY (key text) or IGN_WINVM_KEY_FILE is not set"
      KEYFILE=$TMPD/win_key
      (
        umask 077
        printf '%s' "$IGN_WINVM_KEY" | perl -0pe 's/\r\n/\n/g; s/\r/\n/g; s/\n+\z//; $_ .= "\n"' > "$KEYFILE"
      )
    fi
  fi
  [ -s "$PIN" ] || die "pinned known_hosts file is missing or empty"
  [ -r "$KEYFILE" ] || die "the key file for $VM is not readable"
  grep -q -F "$HOST " "$PIN" || die "the pinned known_hosts file has no entry for the $VM host"
  case "$(stat -f '%Lp' "$PIN" 2>/dev/null || stat -c '%a' "$PIN")" in
    600 | 400 | 644) ;;
    *) die "pinned known_hosts must be 0600 (or 0644)" ;;
  esac
  DEST="$SSH_USER@$HOST"
  OPTS_COMMON=(
    -o BatchMode=yes
    -o StrictHostKeyChecking=yes
    -o "UserKnownHostsFile=$PIN"
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
    -o PermitLocalCommand=no
    -o ControlMaster=no
    -o ControlPath=none
    -o ConnectTimeout=10
    -o ServerAliveInterval=15
    -o ServerAliveCountMax=8
    -o LogLevel=ERROR
  )
  # every ssh call but the tunnel drops all forwardings; the tunnel adds its own
  OPTS=("${OPTS_COMMON[@]}" -o ClearAllForwardings=yes)
  TUN_ARGS=(-N -T -p 22 -i "$KEYFILE" "${OPTS_COMMON[@]}" -o ExitOnForwardFailure=yes)
  SSH_ARGS=(-T -p 22 -i "$KEYFILE" "${OPTS[@]}")
  SFTP_ARGS=(-q -b - -P 22 -i "$KEYFILE" "${OPTS[@]}")
}

# The output filter: host addresses never reach the terminal or the log.
redact() {
  perl -pe 'BEGIN { $| = 1; @h = grep { length } ($ENV{R27_H1}, $ENV{R27_H2}) } for my $h (@h) { s/\Q$h\E/<host>/g }'
}

run_to() { # SECONDS CMD...
  local secs=$1
  shift
  perl -e 'alarm shift; exec @ARGV' "$secs" "$@"
}

lnx_script() { # NAME [VAR=VALUE ...]: the script on stdin of `bash -s`
  local name=$1 kv k v
  shift
  {
    for kv in "$@"; do
      k=${kv%%=*}
      v=${kv#*=}
      printf 'export %s=%q\n' "$k" "$v"
    done
    "R_lnx_$name"
  } | run_to "${T_SSH:-1800}" ssh "${SSH_ARGS[@]}" -- "$DEST" 'bash -s'
}

sftp_batch() {
  printf '%s\n' "$@" | run_to "${T_SFTP:-3600}" sftp "${SFTP_ARGS[@]}" -- "$DEST" | tr -d '\r'
}

win_ps() { # NAME [ARG ...]
  local name=$1 f remote args="" a
  shift
  remote="ign-git27-$name.ps1"
  f=$TMPD/$remote
  win_script "$name" > "$f"
  for a in "$@"; do
    case "$a" in
      *\"* | *\\) die "unsupported PowerShell argument" ;;
    esac
    args="$args \"$a\""
  done
  sftp_batch "put $f $remote" > /dev/null
  local rc=0
  run_to "${T_SSH:-1800}" ssh "${SSH_ARGS[@]}" -- "$DEST" "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File .\\$remote$args" < /dev/null | tr -d '\r' || rc=$?
  sftp_batch "rm $remote" > /dev/null 2>&1 || true
  return "$rc"
}

# tunnel_up: a pinned `ssh -L` from a free port on 127.0.0.1 to the gateway port on the
# VM's loopback (WR-D5). Sets TUN_PORT. A forward that the VM's sshd refuses, or one that
# never answers a token-free request, stops the step: there is no fallback to the VM's
# address, and nothing here changes the VM's sshd or network rules.
tunnel_up() {
  local port deadline pid=""
  tunnel_down
  command -v curl > /dev/null 2>&1 || die "curl is required"
  port=$(perl -MIO::Socket::INET -e 'my $s = IO::Socket::INET->new(Listen => 1, LocalAddr => "127.0.0.1", LocalPort => 0, Proto => "tcp") or die "no free port"; print $s->sockport') || die "no free loopback port"
  case "$port" in '' | *[!0-9]*) die "no free loopback port" ;; esac
  # the tunnel must not hold this step's pipe open: its output goes to a file
  ssh "${TUN_ARGS[@]}" -L "127.0.0.1:$port:127.0.0.1:$GW_PORT" -- "$DEST" < /dev/null > "$TMPD/tunnel.err" 2>&1 &
  pid=$!
  printf '%s\n' "$pid" > "$TUN_PIDFILE"
  deadline=$(($(date +%s) + 30))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if ! kill -0 "$pid" 2> /dev/null; then
      echo "ssh tunnel stderr: $(tail -n 3 "$TMPD/tunnel.err" | tr '\n' ' ')" >&2
      die "the ssh tunnel to the $VM gateway exited (the VM's sshd may refuse the port forward); not falling back to the VM's address"
    fi
    # token-free: the gateway's status ping carries no credential
    if curl -s -o /dev/null --noproxy '*' --max-time 3 "http://127.0.0.1:$port/StatusPing" 2> /dev/null; then
      TUN_PORT=$port
      return 0
    fi
    sleep 1
  done
  echo "ssh tunnel stderr: $(tail -n 3 "$TMPD/tunnel.err" | tr '\n' ' ')" >&2
  tunnel_down
  die "the $VM gateway did not answer through the ssh tunnel within 30 s (the VM's sshd may refuse the port forward); not falling back to the VM's address"
}

tunnel_down() {
  local pid
  if [ -s "$TUN_PIDFILE" ]; then
    pid=$(cat "$TUN_PIDFILE")
    kill "$pid" 2> /dev/null || true
    wait "$pid" 2> /dev/null || true
    : > "$TUN_PIDFILE"
  fi
  TUN_PORT=
}

ign_gw() { # ign args... against the selected VM's gateway, only through the tunnel
  [ -n "$TUN_PORT" ] || die "no ssh tunnel to the $VM gateway: nothing is sent to the gateway without it"
  # bounded: a call that waits on a credential prompt (a keychain dialog) must fail, not hang
  if [ -n "$IGN_CFG" ]; then
    run_to "${T_IGN:-120}" env IGNITION_CLI_CONFIG="$IGN_CFG" IGNITION_URL="http://127.0.0.1:$TUN_PORT" "$IGN" --profile "$PROFILE" --json "$@"
  else
    run_to "${T_IGN:-120}" env IGNITION_URL="http://127.0.0.1:$TUN_PORT" "$IGN" --profile "$PROFILE" --json "$@"
  fi
}

# guard: the target must be the test VM. 1) ssh answers with the pinned key,
# 2) gateway-info at the same address names the expected gateway.
guard() {
  local name
  case "$GW_PORT" in '' | *[!0-9]*) die "gateway port is not a number" ;; esac
  if ! run_to 60 ssh "${SSH_ARGS[@]}" -- "$DEST" whoami < /dev/null > /dev/null 2>&1; then
    die "the $VM host did not answer over ssh with the pinned host key; it may no longer be the test VM (the Windows VM address is dynamic)"
  fi
  tunnel_up
  name=$(ign_gw api call --method GET --path /data/api/v1/gateway-info 2> /dev/null | jq -r '.data.result.data.name // empty' 2> /dev/null || true)
  if [ "$name" != "$GW_NAME" ]; then
    die "gateway-info through the ssh tunnel names '${name:-<nothing>}', not '$GW_NAME' (an empty answer can be a credential prompt, such as a keychain dialog for the ign binary, that nobody answered)"
  fi
  echo "GUARD ok $VM: ssh answered with the pinned key; gateway-info through the ssh tunnel names $GW_NAME"
}

project_present() { # exit 0 when CapApp27 is in the REST project list
  local out
  out=$(ign_gw project list 2>&1) || die "project list failed on the $VM gateway"
  printf '%s' "$out" | jq -e --arg p "$PROJ" '[.. | objects | select(.name? == $p)] | length > 0' > /dev/null
}

each_vm() { # FUNCTION: run it once per selected VM, after the guard
  local v fn=$1
  for v in $VMS; do
    vm_select "$v"
    echo "######## $v: $fn"
    guard
    "$fn"
    tunnel_down
  done
}

# ---------------------------------------------------------------------------
# Steps
# ---------------------------------------------------------------------------

vm_project_up() {
  if project_present; then die "project $PROJ already exists on the $VM gateway; run project-down first"; fi
  ign_gw project new "$PROJ" --title "capture27 disposable" | jq -c '{ok, project: .data}' 2>&1 | head -c 400 || true
  echo
  project_present || die "project $PROJ is not in the project list after create"
  echo "project-list: $PROJ present"
  if [ "$VM" = lnx ]; then lnx_script project_dir "DATA=$DATA" "PROJ=$PROJ"; else win_ps project-state "$DATA" "$PROJ" present; fi
}

step_project_up() { each_vm vm_project_up; }

vm_upload() {
  local out=${PRODSHAPE_GIT_OUT:-} stage=$CACHE/stage
  [ -n "$out" ] || die "PRODSHAPE_GIT_OUT is not set"
  [ -d "$out/capapp.git" ] && [ -d "$out/data/projects/CapApp/.git" ] || die "PRODSHAPE_GIT_OUT does not hold capapp.git and data/projects/CapApp/.git"
  case "$out" in *[[:space:]]*) die "the fixture path holds a space" ;; esac
  mkdir -p "$stage"
  chmod 700 "$stage"
  if [ ! -s "$stage/src.git.tar" ] || [ "${CAPTURE27_REARCHIVE:-0}" = 1 ]; then
    COPYFILE_DISABLE=1 tar --no-xattrs -cf "$stage/src.git.tar" -C "$out/data/projects/CapApp/.git" .
    COPYFILE_DISABLE=1 tar --no-xattrs -cf "$stage/remote.git.tar" -C "$out/capapp.git" .
  fi
  echo "local: src.git $(find "$out/data/projects/CapApp/.git" -type f | wc -l | tr -d ' ') files, remote.git $(find "$out/capapp.git" -type f | wc -l | tr -d ' ') files; archives $(wc -c < "$stage/src.git.tar" | tr -d ' ') and $(wc -c < "$stage/remote.git.tar" | tr -d ' ') bytes"
  local n t0
  for n in src.git remote.git; do
    t0=$(date +%s)
    sftp_batch "put $stage/$n.tar ign-git27-$n.tar" > /dev/null
    echo "uploaded $n.tar in $(($(date +%s) - t0)) s"
    if [ "$VM" = lnx ]; then
      lnx_script upload_extract "SCRATCH_REL=$SCRATCH_REL" "NAME=$n"
    else
      win_ps upload-extract "$n"
    fi
  done
}

step_upload() { each_vm vm_upload; }

vm_g1_before() {
  if [ "$VM" = lnx ]; then
    lnx_script g1_before "DATA=$DATA" "PROJ=$PROJ"
  else
    win_ps g1-before "$DATA" "$PROJ"
  fi
}

step_g1_before() { each_vm vm_g1_before; }

vm_g4() {
  if [ "$VM" = lnx ]; then
    lnx_script g4 "SCRATCH_REL=$SCRATCH_REL" "BUDGET_SAVE=$BUDGET_SAVE" "BUDGET_RV=$BUDGET_RV"
  else
    win_ps g4 "$BUDGET_SAVE" "$BUDGET_RV"
  fi
}

step_g4() { each_vm vm_g4; }

vm_g8() {
  [ "$VM" = win ] || { echo "g8 is a Windows step; nothing to do on $VM"; return 0; }
  win_ps g8 "$DATA" "$PROJ"
  echo "== git module on the gateway (REST module list)"
  ign_gw modules | jq -c '[.. | objects | select(((.id? // .moduleId? // "") | tostring) | test("axone"))]' || true
  ign_gw modules --quarantined | jq -c '[.. | objects | select(((.id? // .moduleId? // "") | tostring) | test("axone"))]' || true
}

step_g8() { each_vm vm_g8; }

vm_g8_jgit() {
  [ "$VM" = win ] || { echo "g8-jgit is a Windows step; nothing to do on $VM"; return 0; }
  win_ps g8-jgit "$DATA" "$PROJG"
}

step_g8_jgit() { each_vm vm_g8_jgit; }

vm_g1_after() {
  if [ "$VM" = lnx ]; then
    lnx_script g1_after "DATA=$DATA" "PROJ=$PROJ" "SCRATCH_REL=$SCRATCH_REL"
  else
    win_ps g1-after "$DATA" "$PROJ" "${CAPTURE27_WIN_TOOL:-robocopy}"
  fi
}

step_g1_after() { each_vm vm_g1_after; }

vm_project_down() {
  if project_present; then
    ign_gw --yes project delete "$PROJ" | jq -c '{ok}' || true
  fi
  if project_present; then die "project $PROJ is still in the project list on the $VM gateway"; fi
  echo "project-list after project-down (names): $(ign_gw project list | jq -r '[.. | objects | .name? | strings] | unique | join(",")')"
  if [ "$VM" = lnx ]; then lnx_script project_gone "DATA=$DATA" "PROJ=$PROJ"; else win_ps project-state "$DATA" "$PROJ" absent; fi
}

step_project_down() { each_vm vm_project_down; }

vm_scratch_down() {
  if [ "$VM" = lnx ]; then lnx_script scratch_down "SCRATCH_REL=$SCRATCH_REL"; else win_ps scratch-down; fi
}

step_scratch_down() { each_vm vm_scratch_down; }

step_guard() { each_vm vm_guard_only; }

vm_guard_only() { echo "guard only: nothing else to do on $VM"; }

# vm-gate TEST: the ignored live gate TEST of the one selected VM, cargo from the repo root
# inside this step's redact-and-tee runner. It passes only on cargo exit 0 plus an anchored
# `LIVE-RAN TEST` line (WR-D7, D-18). The gate reaches the gateway only through the tunnel:
# the driver exports the loopback URL, and the gate refuses anything else (WR-D5).
vm_vm_gate() {
  local bin out=$TMPD/gate.out rc=0 t=$GATE_TEST
  local -a gate_env
  case "$VM" in
    lnx)
      bin=live_transport_lnxvm
      gate_env=(
        IGNITION_LIVE_LNXVM=1
        "IGN_LNXVM_HOST=$HOST"
        "IGN_LNXVM_USER=$SSH_USER"
        "IGN_LNXVM_KEY_FILE=$KEYFILE"
        "IGN_LNXVM_KNOWN_HOSTS=$PIN"
        "IGN_LNXVM_PROFILE=$PROFILE"
        "IGN_LNXVM_DATA_DIR=$DATA"
        "IGN_LNXVM_GATEWAY_NAME=$GW_NAME"
        "IGN_LNXVM_GATEWAY_URL=http://127.0.0.1:$TUN_PORT"
        "IGNITION_CLI_CONFIG=$IGN_CFG"
      )
      ;;
    win)
      bin=live_transport_wincap
      gate_env=(
        IGNITION_LIVE_WINCAP=1
        "IGN_WINCAP_HOST=$HOST"
        "IGN_WINCAP_USER=$SSH_USER"
        "IGN_WINCAP_KEY=${IGN_WINVM_KEY:-$(cat "$KEYFILE")}"
        "IGN_WINCAP_KNOWN_HOSTS=$PIN"
        "IGN_WINCAP_PROFILE=$PROFILE"
        "IGN_WINCAP_DATA_DIR=$DATA"
        "IGN_WINVM_GATEWAY_NAME=$GW_NAME"
        "IGN_WINVM_GATEWAY_URL=http://127.0.0.1:$TUN_PORT"
      )
      ;;
  esac
  command -v cargo > /dev/null 2>&1 || die "cargo is required for vm-gate"
  echo "== vm-gate $t ($bin)"
  (
    cd "$REPO_ROOT"
    env "${gate_env[@]}" cargo test -p ignition-core --test "$bin" "$t" -- --ignored --nocapture --test-threads=1
  ) 2>&1 | tee "$out"
  rc=${PIPESTATUS[0]}
  [ "$rc" -eq 0 ] || die "cargo test $t exited $rc"
  grep -Eq "(^|[[:space:]])LIVE-RAN $t\$" "$out" || die "cargo test $t exited 0 but printed no LIVE-RAN $t line (a skipped gate is not a pass)"
}

step_vm_gate() {
  local -a sel
  read -r -a sel <<< "$VMS"
  [ "${#sel[@]}" -eq 1 ] || die "vm-gate needs exactly one VM in CAPTURE27_VMS (lnx or win)"
  each_vm vm_vm_gate
}

# ---------------------------------------------------------------------------
# Run the step with its log and the redaction filter.
# ---------------------------------------------------------------------------

fn=step_${STEP//-/_}
LOG_STEP=${LOG_STEP:-$STEP}
if [ "$STEP" = vm-gate ] && [ "$LOG_STEP" = vm-gate ]; then
  LOG=$EVID/$RUN-$LOG_STEP-$GATE_TEST.log
else
  LOG=$EVID/$RUN-$LOG_STEP.log
fi
# Pre-resolve the host strings the filter must hide (without network access).
R27_H1=${IGN_LNXVM_HOST:-}
if [ -z "$R27_H1" ] && [ -r "$HOME/.cache/ign-lnxvm01/host" ]; then R27_H1=$(tr -d '[:space:]' < "$HOME/.cache/ign-lnxvm01/host"); fi
R27_H2=${IGN_WINVM_HOST:-}
export R27_H1 R27_H2

printf 'capture27-vm %s started %s\n' "$LOG_STEP" "$(date -u +%FT%TZ)" > "$LOG"
rc=0
set +e
(
  set -e
  "$fn"
) 2>&1 | redact | tee -a "$LOG"
rc=${PIPESTATUS[0]}
set -e
if [ "$rc" -eq 0 ]; then
  echo "LIVE-RAN capture27-vm $LOG_STEP" | tee -a "$LOG"
else
  echo "capture27-vm $LOG_STEP FAILED with exit $rc" | tee -a "$LOG" >&2
fi
exit "$rc"

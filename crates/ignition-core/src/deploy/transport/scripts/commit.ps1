# ign deploy: the on-box batch for a Windows PowerShell 5.1 host (Phase 23,
# plan 23-08, D-01). The commit and restore ops; 23-20 adds the rest.
#
# Run by a fixed stub (deploy/transport/winps.rs, script_stub) that has already
# compared the SHA-256 of this file and of work.tsv with the values the runner
# computed, and that runs this text through [ScriptBlock]::Create, which the
# host's execution policy does not govern (C-script-run). Arguments:
#
#   OP STAGING COLLECTION DEPLOY WORKTEXT
#
# OP is commit, restore, verify or unstage. STAGING holds the files the work
# list names, COLLECTION is <data dir>\config\resources\<mode> and DEPLOY is
# <home>\ign-deploy\<gateway>\<mode>, all native absolute paths. WORKTEXT is the
# text of work.tsv exactly as the stub hashed it, so the list this script acts
# on is the list that was checked.
#
# The script never prints a path or a file's content: its one IGN-RESULT: line
# is ASCII JSON whose per-entry results are indices into the work list (a
# removed directory is [delete index, levels]). Every path is used with the
# \\?\ prefix. This file is pure ASCII.
#
# Operation order and rules are commit.sh's (the POSIX host) and the local
# host's (deploy/apply/host.rs), with the C7a-4 rules of this platform: a file
# is replaced by moving it over its target, never onto itself, and a held
# target is retried a bounded number of times.

param([string]$Op, [string]$St, [string]$Co, [string]$De, [string]$WorkText)
Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

# One table for everything the helpers share, so they read and write it
# without a scope prefix.
$S = @{
    Res = $null; Phase = ''; Run = ''; Gw = ''; Md = ''; WOp = ''
    Retries = 3; JSeen = '-'; HaveJ = $false; HaveL = $false; LNew = ''
    RecDoc = ''; Fail = ''; RootsOk = $false; LPre = '-'; SnapBad = ''; Restored = ''
}
$Ws = New-Object Collections.Generic.List[object]
$Ds = New-Object Collections.Generic.List[object]
$Ps = New-Object Collections.Generic.List[object]
$Fs = New-Object Collections.Generic.List[object]
$Moved = New-Object Collections.Generic.List[int]
$Deleted = New-Object Collections.Generic.List[int]
$Dirs = New-Object Collections.Generic.List[object]
$Stale = New-Object Collections.Generic.List[string]
$Removed = @{}
$Created = New-Object Collections.Generic.List[int]
$NL = "`n"
$TAB = "`t"

# ---- output ------------------------------------------------------------------

# Fin RESULT: the operation is over; the one result line is written by the
# caller at the very end, after the cleanup.
function Fin($o) {
    $S.Res = $o
    throw 'IGN-STOP'
}

# Refuse CODE TEXT [EXTRA]: nothing was written; one result line.
function Refuse([string]$code, [string]$detail, $extra = $null) {
    $o = @{ ok = 0; err = $code; detail = $detail }
    if ($extra) {
        foreach ($k in @($extra.Keys)) { $o[$k] = $extra[$k] }
    }
    Fin $o
}

function MakeReport([string]$phase) {
    $f = $null
    if ($S.Fail) { $f = $S.Fail }
    return @{ ok = 1; phase = $phase; stale = $Stale; moved = $Moved; deleted = $Deleted; dirs = $Dirs; fail = $f }
}

function Report([string]$phase) {
    Fin (MakeReport $phase)
}

# ---- primitives --------------------------------------------------------------

# The extended-length form of a native absolute path.
function Xp([string]$pth) {
    if ($pth.StartsWith('\\?\')) { return $pth }
    return '\\?\' + $pth
}

# BASE\REL with REL's slashes turned around.
function Sub([string]$base, [string]$rel) {
    return $base + '\' + $rel.Replace('/', '\')
}

# The attributes of a path without following anything: -1 absent, -2 unreadable.
function Attr([string]$pth) {
    try {
        return [int][IO.File]::GetAttributes((Xp $pth))
    }
    catch [IO.FileNotFoundException] { return -1 }
    catch [IO.DirectoryNotFoundException] { return -1 }
    catch { return -2 }
}

function IsDir([int]$a) { return ($a -ge 0) -and (($a -band 16) -ne 0) }
function IsLink([int]$a) { return ($a -ge 0) -and (($a -band 1024) -ne 0) }

# The lowercase SHA-256 of a file, or $null when it cannot be read.
function Sha([string]$pth) {
    $fs = $null
    $alg = $null
    try {
        $fs = New-Object IO.FileStream((Xp $pth), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]7)
        $alg = [Security.Cryptography.SHA256]::Create()
        return [BitConverter]::ToString($alg.ComputeHash($fs)).Replace('-', '').ToLowerInvariant()
    }
    catch { return $null }
    finally {
        if ($alg) { $alg.Dispose() }
        if ($fs) { $fs.Dispose() }
    }
}

function IsHex64([string]$t) { return $t -cmatch '^[0-9a-f]{64}$' }
function IsHexOrDash([string]$t) { return ($t -eq '-') -or (IsHex64 $t) }

# One path component on Windows: no separator, drive colon, reserved character
# or control character, not . or .., no trailing dot or space (Windows strips
# them), not a reserved device name.
function CompOk([string]$c) {
    if ($c.Length -eq 0 -or $c -eq '.' -or $c -eq '..') { return $false }
    if ($c -match '[\x00-\x1f\\:<>"|?*/]') { return $false }
    if ($c.EndsWith('.') -or $c.EndsWith(' ')) { return $false }
    if ($c -match '^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$') { return $false }
    return $true
}

# classify BASE REL: absent, file, dir, symlink or unsafe, without following a
# reparse point. A symlink or junction on the way is symlink; a special entry,
# a file where a directory is needed and a directory that cannot be read are
# unsafe.
function Classify([string]$base, [string]$rel) {
    if ((Attr $base) -eq -1) { return 'absent' }
    $cur = $base
    $parts = $rel.Split('/')
    $kind = 'dir'
    for ($i = 0; $i -lt $parts.Length; $i++) {
        $parent = $cur
        $cur = $cur + '\' + $parts[$i]
        $a = Attr $cur
        if ($a -eq -1) {
            if (IsDir (Attr $parent)) { return 'absent' }
            return 'unsafe'
        }
        if ($a -eq -2) { return 'unsafe' }
        if (IsLink $a) { return 'symlink' }
        if (IsDir $a) {
            $kind = 'dir'
        }
        else {
            $kind = 'file'
            if ($i -lt $parts.Length - 1) { return 'unsafe' }
        }
    }
    return $kind
}

# live BASE REL: the SHA-256 of BASE\REL, '-' when absent and '!' when it is
# not a plain regular file reached without a reparse point.
function Live([string]$base, [string]$rel) {
    $k = Classify $base $rel
    if ($k -eq 'absent') { return '-' }
    if ($k -eq 'file') {
        $h = Sha (Sub $base $rel)
        if ($h) { return $h }
    }
    return '!'
}

# An absolute native path of the shape the runner builds: a drive, then plain
# components.
function PlainAbs([string]$pth) {
    if ($pth -notmatch '^[A-Za-z]:\\.') { return $false }
    foreach ($c in $pth.Substring(3).Split('\')) {
        if (-not (CompOk $c)) { return $false }
    }
    return $true
}

# One name that may stand for a gateway, a mode or a run.
function PlainName([string]$n) {
    return (CompOk $n) -and (-not $n.StartsWith('.'))
}

# collection_path REL: <module>/<type>/<name>..., no dot-named or gateway-owned
# component.
function CollectionPath([string]$rel) {
    if ($rel.Length -eq 0 -or $rel.StartsWith('/') -or $rel.EndsWith('/')) { return $false }
    $parts = $rel.Split('/')
    if ($parts.Length -lt 3) { return $false }
    foreach ($c in $parts) {
        if (-not (CompOk $c)) { return $false }
        if ($c.StartsWith('.') -or $c -eq 'config-mode.json') { return $false }
    }
    return $true
}

function DepthOf([string]$rel) { return $rel.Split('/').Length }

function HasEntries([string]$dir) {
    $en = [IO.Directory]::EnumerateFileSystemEntries((Xp $dir)).GetEnumerator()
    try { return $en.MoveNext() } finally { $en.Dispose() }
}

# Replace DEST with SRC, which stays only on a failure. The target is replaced
# by moving over it (C7a-4); it never moves onto itself.
function MoveReplace([string]$src, [string]$dst) {
    if ((Xp $src) -ieq (Xp $dst)) { throw 'a file cannot move onto itself' }
    Move-Item -LiteralPath (Xp $src) -Destination (Xp $dst) -Force
}

# replace_with SRC DEST: copy SRC beside DEST and move the copy over DEST.
function ReplaceWith([string]$src, [string]$dst) {
    $tmp = $dst + '.ign-tmp'
    try {
        if ([IO.File]::Exists((Xp $tmp))) { [IO.File]::Delete((Xp $tmp)) }
        [IO.File]::Copy((Xp $src), (Xp $tmp))
        MoveReplace $tmp $dst
        return $true
    }
    catch {
        try { if ([IO.File]::Exists((Xp $tmp))) { [IO.File]::Delete((Xp $tmp)) } } catch { }
        return $false
    }
}

function RemoveTree([string]$pth) {
    if ((Attr $pth) -ge 0) { Remove-Item -LiteralPath (Xp $pth) -Recurse -Force }
}

# ---- the work list --------------------------------------------------------------

function CheckWork {
    if ($WorkText.Contains("`r")) { Refuse 'bad_work' 'the work list holds a carriage return' }
    $lines = $WorkText.Split("`n")
    if ($lines.Length -lt 2 -or $lines[$lines.Length - 1] -ne '') { Refuse 'bad_work' 'the work list does not end with a newline' }
    if ($lines[0] -cne 'ign-work 1') { Refuse 'bad_work' 'the work list header is wrong' }
    $haveR = $false; $haveG = $false; $haveM = $false; $haveO = $false; $haveT = $false
    $arity = @{ R = 2; G = 2; M = 2; O = 2; T = 2; J = 2; F = 3; W = 5; D = 4; L = 3; P = 4 }
    for ($n = 1; $n -lt $lines.Length - 1; $n++) {
        $f = $lines[$n].Split("`t")
        $k = $f[0]
        if ($k -cnotmatch '^[RGMOTJFWDLP]$') { Refuse 'bad_work' 'the work list holds a record this script does not know' }
        if ($f.Length -ne $arity[$k]) { Refuse 'bad_work' 'a record has the wrong number of fields' }
        switch -CaseSensitive ($k) {
            'R' { if ($haveR -or $f[1] -eq '') { Refuse 'bad_work' 'a run record is repeated' }; $haveR = $true; $S.Run = $f[1] }
            'G' { if ($haveG -or $f[1] -eq '') { Refuse 'bad_work' 'a gateway record is repeated' }; $haveG = $true; $S.Gw = $f[1] }
            'M' { if ($haveM -or $f[1] -eq '') { Refuse 'bad_work' 'a mode record is repeated' }; $haveM = $true; $S.Md = $f[1] }
            'O' { if ($haveO -or $f[1] -eq '') { Refuse 'bad_work' 'an operation record is repeated' }; $haveO = $true; $S.WOp = $f[1] }
            'T' {
                if ($haveT) { Refuse 'bad_work' 'a retry record is repeated' }
                $haveT = $true
                if ($f[1] -notmatch '^[0-9]{1,2}$' -or [int]$f[1] -gt 10) { Refuse 'bad_work' 'the retry count is wrong' }
                $S.Retries = [int]$f[1]
            }
            'J' {
                if ($S.HaveJ) { Refuse 'bad_work' 'a journal record is repeated' }
                $S.HaveJ = $true
                if (-not (IsHexOrDash $f[1])) { Refuse 'bad_work' 'the journal hash is wrong' }
                $S.JSeen = $f[1]
            }
            'F' {
                if ($f[1] -cnotmatch '^(journal\.(snapshotting|ready|writing|written|restoring)|record\.(applying|applied|rolled_back))\.json$') { Refuse 'bad_work' 'a staged file name is wrong' }
                if (-not (IsHex64 $f[2])) { Refuse 'bad_work' 'a staged file hash is wrong' }
                if ($f[1].StartsWith('record.')) { $S.RecDoc = $f[1] }
                $Fs.Add(@{ name = $f[1]; sha = $f[2] })
            }
            'W' {
                if ($f[1] -ne [string]$Ws.Count) { Refuse 'bad_work' 'a write index is wrong' }
                if (-not (IsHex64 $f[2])) { Refuse 'bad_work' 'a write hash is wrong' }
                if (-not (IsHexOrDash $f[3])) { Refuse 'bad_work' 'a write pre-image hash is wrong' }
                if (-not (CollectionPath $f[4])) { Refuse 'bad_work' 'a write path is wrong' }
                $Ws.Add(@{ i = $Ws.Count; sha = $f[2]; pre = $f[3]; path = $f[4] })
            }
            'D' {
                if ($f[1] -ne [string]$Ds.Count) { Refuse 'bad_work' 'a delete index is wrong' }
                if (-not (IsHex64 $f[2])) { Refuse 'bad_work' 'a delete pre-image hash is wrong' }
                if (-not (CollectionPath $f[3])) { Refuse 'bad_work' 'a delete path is wrong' }
                $Ds.Add(@{ i = $Ds.Count; pre = $f[2]; path = $f[3] })
            }
            'L' {
                if ($S.HaveL) { Refuse 'bad_work' 'a ledger record is repeated' }
                if (-not (IsHex64 $f[1])) { Refuse 'bad_work' 'the ledger hash is wrong' }
                if (-not (IsHexOrDash $f[2])) { Refuse 'bad_work' 'the prior ledger hash is wrong' }
                $S.HaveL = $true
                $S.LNew = $f[1]
                $S.LPre = $f[2]
            }
            'P' {
                if ($f[1] -ne [string]$Ps.Count) { Refuse 'bad_work' 'a restore index is wrong' }
                if (-not (IsHexOrDash $f[2])) { Refuse 'bad_work' 'a restore hash is wrong' }
                if ($f[3] -cne 'ledger.json' -and -not (CollectionPath $f[3])) { Refuse 'bad_work' 'a restore path is wrong' }
                $Ps.Add(@{ i = $Ps.Count; pre = $f[2]; path = $f[3] })
            }
        }
    }
    if (-not ($haveR -and $haveG -and $haveM -and $haveO)) { Refuse 'bad_work' 'the work list is missing a record' }
    # verify reads the commit list a commit is about to run
    if (-not (($Op -eq 'verify' -and $S.WOp -eq 'commit') -or $S.WOp -eq $Op)) { Refuse 'bad_work' 'the work list is for another operation' }
    if (-not (PlainName $S.Md)) { Refuse 'bad_work' 'the mode is wrong' }
    if (-not ((PlainName $S.Run) -and (PlainName $S.Gw))) { Refuse 'bad_work' 'the run or gateway is wrong' }
    if ($S.Run -cnotmatch '^[A-Za-z0-9._-]{1,128}$') { Refuse 'bad_work' 'the run id is wrong' }
    if (($Op -eq 'restore') -and (-not $S.HaveJ)) { Refuse 'bad_work' 'the work list does not say which journal was seen' }
}

# check_roots: the three roots are plain absolute paths of the shape the runner
# builds for this work list, and none of the directories this script creates
# inside is a reparse point.
function CheckRoots {
    if (-not ((PlainAbs $St) -and (PlainAbs $Co) -and (PlainAbs $De))) { Refuse 'bad_roots' 'a root is not a plain absolute path' }
    $S.RootsOk = $true
    if (-not $Co.EndsWith('\config\resources\' + $S.Md, [StringComparison]::OrdinalIgnoreCase)) { Refuse 'bad_roots' 'the collection is not the mode collection' }
    if (IsLink (Attr $Co)) { Refuse 'unsafe' 'the mode collection is a reparse point' }
    if (-not $De.EndsWith('\ign-deploy\' + $S.Gw + '\' + $S.Md, [StringComparison]::OrdinalIgnoreCase)) { Refuse 'bad_roots' 'the deploy directory is not this gateway and mode' }
    $d = $De
    for ($n = 0; $n -lt 3; $n++) {
        if (IsLink (Attr $d)) { Refuse 'unsafe' 'the deploy directory is or is below a reparse point' }
        $d = $d.Substring(0, $d.LastIndexOf('\'))
    }
    if ($St.StartsWith($Co + '\', [StringComparison]::OrdinalIgnoreCase) -or $Co.StartsWith($St + '\', [StringComparison]::OrdinalIgnoreCase)) { Refuse 'bad_roots' 'the staging directory and the collection overlap' }
    if ($Op -eq 'commit' -or $Op -eq 'verify') {
        if ($St -ine ($De + '\staging\' + $S.Run)) { Refuse 'bad_roots' 'the staging directory is not this run''s' }
    }
}

# A control directory (.ign-ctl-*, made by the runner for an operation that has
# no staging directory of its own) holds this script and its work list; it is
# removed when the script exits, whatever the outcome.
function CtlCleanup {
    if (-not (PlainAbs $St)) { return }
    $leaf = $St.Substring($St.LastIndexOf('\') + 1)
    if ($leaf.StartsWith('.ign-ctl-')) {
        $a = Attr $St
        if ((IsDir $a) -and -not (IsLink $a)) {
            try { Remove-Item -LiteralPath (Xp $St) -Recurse -Force } catch { }
        }
    }
}

# ---- staged files ------------------------------------------------------------------

# verify_staged: every pre-rendered file, staged copy and the staged ledger is
# a regular file with the hash the work list gives. The indices of those that
# are not are in $Bad.
function VerifyStaged {
    $bad = New-Object Collections.Generic.List[string]
    $src = $St + '\files'
    foreach ($f in $Fs) {
        $pth = $St + '\' + $f.name
        if ((IsLink (Attr $pth)) -or ((Sha $pth) -cne $f.sha)) { $bad.Add('F' + $f.name) }
    }
    foreach ($w in $Ws) {
        if (((Classify $src $w.path) -ne 'file') -or ((Sha (Sub $src $w.path)) -cne $w.sha)) { $bad.Add('W' + $w.i) }
    }
    if ($S.HaveL) {
        $pth = $St + '\ledger.json'
        if ((IsLink (Attr $pth)) -or ((Sha $pth) -cne $S.LNew)) { $bad.Add('L') }
    }
    return , $bad
}

function RequireStaged {
    $bad = VerifyStaged
    if ($bad.Count -gt 0) { Refuse 'staged' 'a staged file does not match the work list' @{ bad = $bad } }
}

# ---- journal and record installs ----------------------------------------------------------

# create_journal SRC: put SRC at DEPLOY\journal.json, whole or not at all.
# 0 created, 1 one is already there, 2 it could not be created.
function CreateJournal([string]$src) {
    $j = $De + '\journal.json'
    if ((Attr $j) -ne -1) { return 1 }
    $tmp = $De + '\journal.json.ign-new'
    try {
        if ([IO.File]::Exists((Xp $tmp))) { [IO.File]::Delete((Xp $tmp)) }
        [IO.File]::Copy((Xp $src), (Xp $tmp))
        # a move onto an existing name fails, which makes the create exclusive
        [IO.File]::Move((Xp $tmp), (Xp $j))
        return 0
    }
    catch {
        try { if ([IO.File]::Exists((Xp $tmp))) { [IO.File]::Delete((Xp $tmp)) } } catch { }
        if ((Attr $j) -ne -1) { return 1 }
        return 2
    }
}

# abort_before_move: undo a commit that has not moved a file.
function AbortBeforeMove {
    $snaps = $De + '\snapshots'
    if ((Attr $snaps) -ge 0 -and -not (IsLink (Attr $snaps))) {
        RemoveTree ($snaps + '\' + $S.Run)
    }
    $j = $De + '\journal.json'
    if ((Attr $j) -ge 0) { [IO.File]::Delete((Xp $j)) }
}

# journal_unchanged: the live journal is what the runner saw when it asked for
# this operation (its hash, or absent), so a journal that appeared or changed
# since then is never overwritten or removed on stale information.
function JournalUnchanged {
    $j = $De + '\journal.json'
    $a = Attr $j
    if ($a -ne -1) {
        if (IsLink $a) { return $false }
        $h = Sha $j
        return ($h -and ($h -ceq $S.JSeen))
    }
    return ($S.JSeen -eq '-')
}

# ---- commit ------------------------------------------------------------------------------

# stale_check: fill $Stale with the entries whose live state is not the pre-image.
function StaleCheck {
    $Stale.Clear()
    foreach ($w in $Ws) {
        if ((Live $Co $w.path) -cne $w.pre) { $Stale.Add('W' + $w.i) }
    }
    foreach ($d in $Ds) {
        if ((Live $Co $d.path) -cne $d.pre) { $Stale.Add('D' + $d.i) }
    }
    if ($S.HaveL) {
        if ((Live $De 'ledger.json') -cne $S.LPre) { $Stale.Add('L') }
    }
    return ($Stale.Count -eq 0)
}

# snapshot_one BASE REL PRE DEST: copy BASE\REL to DEST and verify it. 1 when
# the live file is not the pre-image, 2 on an error.
function SnapshotOne([string]$base, [string]$rel, [string]$pre, [string]$dest) {
    if ((Classify $base $rel) -ne 'file') { return 1 }
    if ((Attr $dest) -ne -1) { return 2 }
    try {
        [void][IO.Directory]::CreateDirectory((Xp ($dest.Substring(0, $dest.LastIndexOf('\')))))
        [IO.File]::Copy((Xp (Sub $base $rel)), (Xp $dest))
    }
    catch { return 1 }
    $h = Sha $dest
    if (-not $h) { return 2 }
    if ($h -cne $pre) { return 1 }
    return 0
}

# snapshot_all: copy each existing pre-image and the prior ledger. 0 done, 1
# stale (the entry is in $S.SnapBad), 2 error.
function SnapshotAll {
    $S.SnapBad = ''
    $snaps = $De + '\snapshots'
    if ((Attr $snaps) -ne -1) {
        $a = Attr $snaps
        if ((-not (IsDir $a)) -or (IsLink $a)) { return 2 }
    }
    $root = $snaps + '\' + $S.Run
    try { [void][IO.Directory]::CreateDirectory((Xp $root)) } catch { return 2 }
    foreach ($w in $Ws) {
        if ($w.pre -ne '-') {
            $S.SnapBad = 'W' + $w.i
            $r = SnapshotOne $Co $w.path $w.pre (Sub ($root + '\files') $w.path)
            if ($r -ne 0) { return $r }
        }
    }
    foreach ($d in $Ds) {
        $S.SnapBad = 'D' + $d.i
        $r = SnapshotOne $Co $d.path $d.pre (Sub ($root + '\files') $d.path)
        if ($r -ne 0) { return $r }
    }
    if ($S.HaveL -and $S.LPre -ne '-') {
        $S.SnapBad = 'L'
        $r = SnapshotOne $De 'ledger.json' $S.LPre ($root + '\ledger.json')
        if ($r -ne 0) { return $r }
    }
    return 0
}

# make_parents REL: create the directories above CO\REL that are missing.
function MakeParents([string]$rel) {
    $parts = $rel.Split('/')
    $cur = $Co
    for ($i = 0; $i -lt $parts.Length - 1; $i++) {
        $cur = $cur + '\' + $parts[$i]
        $a = Attr $cur
        if ($a -eq -2) { return $false }
        if ($a -ge 0) {
            if (IsLink $a) { return $false }
            if (IsDir $a) { continue }
            return $false
        }
        try { [void][IO.Directory]::CreateDirectory((Xp $cur)) } catch { return $false }
    }
    return $true
}

# ensure_collection: the mode collection exists (created when missing).
function EnsureCollection {
    $a = Attr $Co
    if (IsLink $a) { return $false }
    if (IsDir $a) { return $true }
    if ($a -ne -1) { return $false }
    $res = $Co.Substring(0, $Co.LastIndexOf('\'))
    $ra = Attr $res
    if ((-not (IsDir $ra)) -or (IsLink $ra)) { return $false }
    try { [void][IO.Directory]::CreateDirectory((Xp $Co)); return $true } catch { return $false }
}

# MoveWithRetries SRC DST: move SRC over DST, retrying a held target a bounded
# number of times (C7a-4). True when it moved.
function MoveWithRetries([string]$src, [string]$dst) {
    $attempt = 0
    while ($true) {
        try {
            MoveReplace $src $dst
            return $true
        }
        catch {
            $attempt++
            if ($attempt -gt $S.Retries) { return $false }
            Start-Sleep -Seconds 1
        }
    }
}

# move_one INDEX REL SHA: move the staged copy over the target. Sets $S.Fail and
# returns $false when it cannot.
function MoveOne([int]$i, [string]$rel, [string]$sha) {
    $src = $St + '\files'
    $k = Classify $Co $rel
    if ($k -ne 'absent' -and $k -ne 'file') {
        $S.Fail = "write ${i}: the target is no longer a regular file"
        return $false
    }
    if (((Classify $src $rel) -ne 'file') -or ((Sha (Sub $src $rel)) -cne $sha)) {
        $S.Fail = "write ${i}: the staged copy changed"
        return $false
    }
    if (-not (MakeParents $rel)) {
        $S.Fail = "write ${i}: cannot create the directories"
        return $false
    }
    if (-not (MoveWithRetries (Sub $src $rel) (Sub $Co $rel))) {
        $S.Fail = "write ${i}: cannot move the file into place"
        return $false
    }
    if ((Sha (Sub $Co $rel)) -cne $sha) {
        $S.Fail = "write ${i}: the file in place does not match its hash"
        return $false
    }
    return $true
}

function Moves {
    foreach ($w in $Ws) {
        if (-not (MoveOne $w.i $w.path $w.sha)) { return $false }
        $Moved.Add($w.i)
    }
    return $true
}

# rmdir_up INDEX REL: remove the directories the removal of REL emptied,
# deepest first, never <module>/<type> or above.
function RmdirUp([int]$i, [string]$rel, [string]$what) {
    $dir = $rel.Substring(0, $rel.LastIndexOf('/'))
    $depth = DepthOf $dir
    while ($depth -ge 3) {
        if (-not $Removed.ContainsKey($dir.ToLowerInvariant())) {
            $k = Classify $Co $dir
            if ($k -eq 'dir') {
                $full = Sub $Co $dir
                $gone = $false
                try { [IO.Directory]::Delete((Xp $full)); $gone = $true } catch { }
                if ($gone) {
                    $Removed[$dir.ToLowerInvariant()] = $true
                    $Dirs.Add(@($i, $depth))
                }
                elseif (HasEntries $full) {
                    return $true
                }
                else {
                    $S.Fail = "${what}: cannot remove an emptied directory"
                    return $false
                }
            }
            elseif ($k -ne 'absent') {
                $S.Fail = "${what}: an emptied directory is not a real directory"
                return $false
            }
        }
        $dir = $dir.Substring(0, [Math]::Max($dir.LastIndexOf('/'), 0))
        $depth--
    }
    return $true
}

# deletes: step 7. The delete set, then the directories it emptied.
function Deletes {
    foreach ($d in $Ds) {
        if ((Classify $Co $d.path) -ne 'file') {
            $S.Fail = "delete $($d.i): the target is no longer a regular file"
            return $false
        }
        try { [IO.File]::Delete((Xp (Sub $Co $d.path))) }
        catch {
            $S.Fail = "delete $($d.i): cannot delete the file"
            return $false
        }
        $Deleted.Add($d.i)
    }
    foreach ($d in $Ds) {
        if (-not (RmdirUp $d.i $d.path "delete $($d.i)")) { return $false }
    }
    return $true
}

# swap_ledger: step 8, last.
function SwapLedger {
    if (-not $S.HaveL) { return $true }
    $k = Classify $De 'ledger.json'
    if ($k -ne 'absent' -and $k -ne 'file') {
        $S.Fail = 'cannot swap the ledger'
        return $false
    }
    if ((Sha ($St + '\ledger.json')) -cne $S.LNew) {
        $S.Fail = 'the staged ledger changed'
        return $false
    }
    if (-not (ReplaceWith ($St + '\ledger.json') ($De + '\ledger.json'))) {
        $S.Fail = 'cannot swap the ledger'
        return $false
    }
    return $true
}

function OpCommit {
    CheckWork
    CheckRoots
    # read-only: the staged files are what the list says
    RequireStaged
    # 1. the journal, exclusively; an existing one refuses
    $created = CreateJournal ($St + '\journal.snapshotting.json')
    if ($created -eq 1) { Refuse 'journal_present' 'a journal of another run is in place' }
    if ($created -ne 0) { Refuse 'journal' 'the journal could not be created' }
    $S.Phase = 'snapshotting'
    # 2. every touched path against its pre-image
    if (-not (StaleCheck)) {
        AbortBeforeMove
        Report 'snapshotting'
    }
    # 3. snapshot
    $snap = SnapshotAll
    if ($snap -eq 1) {
        $Stale.Clear()
        $Stale.Add($S.SnapBad)
        AbortBeforeMove
        Report 'snapshotting'
    }
    if ($snap -ne 0) {
        AbortBeforeMove
        Refuse 'snapshot' 'the snapshot could not be made'
    }
    # 4. the record, at state applying
    $rec = $De + '\snapshots\' + $S.Run + '\record.json'
    $recOk = $false
    try {
        if ((Attr $rec) -eq -1) {
            [IO.File]::Copy((Xp ($St + '\record.applying.json')), (Xp $rec))
            $recOk = $true
        }
    }
    catch { }
    if (-not $recOk) {
        AbortBeforeMove
        Refuse 'record' 'the run record could not be written'
    }
    # 5. phase ready, then writing
    if (-not (ReplaceWith ($St + '\journal.ready.json') ($De + '\journal.json'))) {
        AbortBeforeMove
        Refuse 'journal' 'the journal could not be advanced'
    }
    $S.Phase = 'ready'
    if (-not (ReplaceWith ($St + '\journal.writing.json') ($De + '\journal.json'))) {
        AbortBeforeMove
        Refuse 'journal' 'the journal could not be advanced'
    }
    $S.Phase = 'writing'
    if (-not (EnsureCollection)) {
        $S.Fail = 'the mode collection could not be created'
        Report 'writing'
    }
    # 6. move each staged file over its target
    if (-not (Moves)) { Report 'writing' }
    # 7. the delete set, then the directories it emptied
    if (-not (Deletes)) { Report 'writing' }
    # 8. the ledger, last
    if (-not (SwapLedger)) { Report 'writing' }
    # 9. phase written
    if (-not (ReplaceWith ($St + '\journal.written.json') ($De + '\journal.json'))) {
        $S.Fail = 'cannot write journal phase written'
        Report 'writing'
    }
    Report 'written'
}

# ---- restore ------------------------------------------------------------------------------

# restore_one INDEX PATH PRE: put one path back. Sets $S.Fail (a value-free
# reason) and returns $false when it cannot. $S.Restored becomes put or removed.
function RestoreOne([int]$i, [string]$pth, [string]$pre) {
    $S.Restored = 'put'
    $isLedger = ($pth -ceq 'ledger.json')
    if ($isLedger) {
        $base = $De
        $snapRel = 'ledger.json'
    }
    else {
        $base = $Co
        $snapRel = 'files/' + $pth
    }
    $snapRoot = $De + '\snapshots\' + $S.Run
    $live = Classify $base $pth
    if ($pre -eq '-') {
        if ($live -eq 'absent') { $S.Restored = 'removed'; return $true }
        if ($live -eq 'file') {
            try { [IO.File]::Delete((Xp (Sub $base $pth))) }
            catch {
                $S.Fail = "restore ${i}: cannot remove the file"
                return $false
            }
            $S.Restored = 'removed'
            return $true
        }
        $S.Fail = "restore ${i}: the live path is not a regular file"
        return $false
    }
    if ($live -ne 'absent' -and $live -ne 'file') {
        $S.Fail = "restore ${i}: the live path is not a regular file"
        return $false
    }
    $snapFile = Sub $snapRoot $snapRel
    if ((Classify $snapRoot $snapRel) -ne 'file') {
        $S.Fail = "restore ${i}: the snapshot has no copy of the file"
        return $false
    }
    $sh = Sha $snapFile
    if (-not $sh) {
        $S.Fail = "restore ${i}: the snapshot has no copy of the file"
        return $false
    }
    if ($sh -cne $pre) {
        $S.Fail = "restore ${i}: the snapshot copy does not match its recorded hash"
        return $false
    }
    # already back (a second restore): nothing to write
    if ($live -eq 'file' -and ((Sha (Sub $base $pth)) -ceq $pre)) { return $true }
    if ($isLedger) {
        if (-not (ReplaceWith $snapFile ($De + '\ledger.json'))) {
            $S.Fail = "restore ${i}: cannot put the ledger back"
            return $false
        }
    }
    else {
        if (-not (MakeParents $pth)) {
            $S.Fail = "restore ${i}: cannot create the directories"
            return $false
        }
        $tmp = $St + '\restore.ign-tmp'
        try {
            if ([IO.File]::Exists((Xp $tmp))) { [IO.File]::Delete((Xp $tmp)) }
            [IO.File]::Copy((Xp $snapFile), (Xp $tmp))
        }
        catch {
            $S.Fail = "restore ${i}: cannot stage the restore copy"
            return $false
        }
        if (-not (MoveWithRetries $tmp (Sub $Co $pth))) {
            try { if ([IO.File]::Exists((Xp $tmp))) { [IO.File]::Delete((Xp $tmp)) } } catch { }
            $S.Fail = "restore ${i}: cannot move the file into place"
            return $false
        }
    }
    if ((Sha (Sub $base $pth)) -cne $pre) {
        $S.Fail = "restore ${i}: the restored file does not match its recorded hash"
        return $false
    }
    return $true
}

# restore_pass WANT_LEDGER: visit the P records that are (or are not) the ledger.
function RestorePass([bool]$ledger) {
    foreach ($p in $Ps) {
        if (($p.path -ceq 'ledger.json') -eq $ledger) {
            if (-not (RestoreOne $p.i $p.path $p.pre)) { return $false }
            if ($S.Restored -eq 'put') {
                $Moved.Add($p.i)
            }
            else {
                $Deleted.Add($p.i)
                if (-not $ledger) { $Created.Add($p.i) }
            }
        }
    }
    return $true
}

function OpRestore {
    CheckWork
    CheckRoots
    RequireStaged
    $snapRoot = $De + '\snapshots\' + $S.Run
    $sa = Attr ($De + '\snapshots')
    $ra = Attr $snapRoot
    if ((-not (IsDir $ra)) -or (IsLink $sa) -or (IsLink $ra)) { Refuse 'no_snapshot' 'the run has no snapshot on this host' }
    if (-not (JournalUnchanged)) { Refuse 'journal_changed' 'the journal changed since it was read' }
    if (-not (ReplaceWith ($St + '\journal.restoring.json') ($De + '\journal.json'))) { Refuse 'journal' 'the journal could not be set to restoring' }
    $S.Phase = 'restoring'
    $needs = $false
    foreach ($p in $Ps) {
        if ($p.pre -ne '-' -and $p.path -cne 'ledger.json') { $needs = $true }
    }
    if ($needs -and -not (EnsureCollection)) {
        $S.Fail = 'the mode collection could not be created'
        Report 'restoring'
    }
    # collection files first, the ledger last
    if (-not (RestorePass $false)) { Report 'restoring' }
    if (-not (RestorePass $true)) { Report 'restoring' }
    $ca = Attr $Co
    if ((IsDir $ca) -and -not (IsLink $ca)) {
        foreach ($i in $Created) {
            $p = $Ps[$i]
            if (-not (RmdirUp $p.i $p.path 'removing an emptied directory failed')) { Report 'restoring' }
        }
    }
    Report 'restoring'
}

function OpVerify {
    CheckWork
    CheckRoots
    RequireStaged
    Fin @{ ok = 1 }
}

function OpUnstage {
    CheckWork
    CheckRoots
    $stg = $De + '\staging'
    if ((Attr $stg) -ne -1) {
        $a = Attr $stg
        if ((-not (IsDir $a)) -or (IsLink $a)) { Refuse 'unsafe' 'the staging area is not a real directory' }
        RemoveTree ($stg + '\' + $S.Run)
    }
    Fin @{ ok = 1 }
}

# ---- main ----------------------------------------------------------------------------------

try {
    switch ($Op) {
        'commit' { OpCommit }
        'restore' { OpRestore }
        'verify' { OpVerify }
        'unstage' { OpUnstage }
        default { Refuse 'bad_op' 'this script does not know the operation' }
    }
}
catch {
    if ($_.Exception.Message -ne 'IGN-STOP') {
        if ($S.Phase -eq 'snapshotting' -or $S.Phase -eq 'ready') {
            # nothing has moved: undo what was started and fail the call
            try { AbortBeforeMove } catch { }
            throw
        }
        elseif ($S.Phase) {
            # a failure this script did not foresee after the first move: the
            # report says where it stopped, the journal stays
            $S.Fail = 'the script stopped on an unexpected error'
            $S.Res = MakeReport $S.Phase
        }
        else {
            throw
        }
    }
}
finally {
    CtlCleanup
}
if ($S.Res) { E $S.Res }

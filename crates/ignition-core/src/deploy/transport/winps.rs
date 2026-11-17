//! PowerShell on a Windows OpenSSH host (22-13), built only from the C7a
//! captures in `22-LIVE-CAPTURES.md`.
//!
//! sftp cannot do three things the Windows read and write halves need, so a
//! small, fixed set of scripts runs through
//! [`RemoteShell::exec_powershell`](super::remote::RemoteShell::exec_powershell):
//!
//! - [`ENUMERATE`]: a complete walk of a subtree with the `\\?\` prefix.
//!   The sftp-server resolves no path of 260 characters or more and silently
//!   omits such entries from listings (C7a-8), `get -R` follows junctions
//!   (C7a-5), and it costs about 0.8 s per folder listed, so an sftp listing
//!   is neither complete nor affordable on this host. The walk returns every
//!   entry with its size and reparse-point flag in one call per budget chunk,
//!   and it is the read's listing: sftp only downloads the files it names
//!   (quick 261104-s3n).
//! - [`ENUMERATE_CHILDREN`]: the same columns for the direct children of a
//!   directory only, never opening a child. A read lists the folders on the
//!   path to its requested trees with it, all in one call per budget chunk,
//!   and the names-only provider listing uses it, so that a huge generated
//!   provider tree is never walked.
//! - [`BUNDLE`]: zip exactly the files of an uploaded, hash-checked read list
//!   into one archive in the read's staging dir below the account home
//!   (quick 261105-dl1), so a Windows read moves its files with one sftp
//!   `get` instead of one per file. [`BUNDLE_CLEAN`] removes that staging
//!   dir after a failed read.
//! - [`MOVE_REPLACE`]: `Move-Item -Force`, the documented fallback for
//!   replacing a file when sftp `rename` does not (C7a-6). On the captured host
//!   `rename` over an existing file replaces it (C7a-4), so the Windows commit
//!   uses `rename`; this script is the proven alternative, exercised live.
//! - [`RECORD_REPLACE`]: the atomic replace of the release record (25-20,
//!   G-25-4), one `MoveFileEx(REPLACE_EXISTING | WRITE_THROUGH)` call.
//! - [`GIT_ENTRY`]: the `.git` probe of `deploy projects` (25-08, K9), one
//!   `Get-Item -Force` per level, so a Hidden entry is seen and a denied one is
//!   an error rather than "absent".
//! - The git host channel (Phase 27, 27-09), one script per operation, every
//!   path and name in `$p`: [`GIT_STATE`] (the fixed files in one pass),
//!   [`GIT_SAVE`] and [`GIT_RESTORE`] (one host-local `robocopy` each, with the
//!   manifest compare inside the script), [`GIT_MANIFEST`], [`GIT_SIZE`] (one
//!   enumeration, no file read), [`GIT_DIGEST`], [`GIT_OBJECTS`] (the object files of a gateway `.git`, 27-11), [`GIT_LOCK`] and
//!   [`GIT_REPLACE`] (the lock-and-rename write; the sftp client cannot create
//!   a file exclusively), [`GIT_PROBE`], [`GIT_SAVE_RM`] and [`GIT_FREE`]. They
//!   run under Windows PowerShell 5.1 (the deploy account's ssh session) and
//!   PowerShell 7 (the local tests), so they use neither `??` nor the ternary
//!   operator. Each is built from shared pieces by `macro_rules!` so the text a
//!   test runs is the text a host receives, and each must fit
//!   [`COMMAND_BUDGET`] with realistic maximal parameters (the encoded command
//!   is about 2.7 times the script text: one script is about 2,400 characters
//!   at most); `winps_git_scripts_fit_budget` pins that.
//!
//! ## How a command is built
//!
//! [`encoded_command`] assembles `prelude + parameter decode + body` and hands
//! the text to [`RemoteCommand::powershell_encoded`], the one UTF-16LE base64
//! encoder; there is no second encoder and no way to pass a raw command line.
//! The bodies are `const` strings. Parameters travel only as a base64 JSON
//! blob decoded inside the script (`$p`), never by string interpolation, so a
//! path with a quote, a `$` or a backtick is data. The whole remote command
//! is held to [`COMMAND_BUDGET`] characters before anything spawns (C7a-7: it
//! works to 8154, fails at 8162).
//!
//! ## What the output means
//!
//! - The prelude sets `$ErrorActionPreference='Stop'` (without it a failed
//!   step exits 0, C7a-6) and `$ProgressPreference='SilentlyContinue'`
//!   (without it progress CLIXML reaches stderr). Error CLIXML still reaches
//!   stderr on a failure even so, so **stderr is opaque**: success is decided
//!   from the exit code and exactly one [`IGN_RESULT_SENTINEL`] line on
//!   stdout, and stderr is decoded only to explain a failure.
//! - The sentinel line carries compact JSON, escaped to ASCII: PowerShell
//!   writes stdout in the OEM code page 437 (C7a-6), so non-ASCII text is
//!   `\uXXXX` before it leaves the host.
//! - An error record on stderr with exit 0 is a failure too: with `Stop` no
//!   error should survive, so one that does means a step did not do what the
//!   script says.

use serde_json::Value;

use super::process::ProcessOutput;
use super::remote::{RemoteCommand, scrub_stderr};

/// The marker that starts the one stdout line a script's result travels on.
pub const IGN_RESULT_SENTINEL: &str = "IGN-RESULT:";

/// The longest remote command, in characters, that is sent (C7a-7: 8154 works,
/// 8162 fails with "The command line is too long."; 8000 leaves a margin).
pub const COMMAND_BUDGET: usize = 8000;

/// `\\?\`: the extended-length path prefix the enumeration walks with.
pub const LONG_PATH_PREFIX: &str = "\\\\?\\";

/// Entries whose absolute path has this many characters or more are refused:
/// the sftp-server cannot resolve a path of 260 characters (C7a-8), and a
/// margin keeps a path that grows by a rename from crossing it unseen.
pub const LONG_PATH_LIMIT: usize = 240;

/// Run before every script. `Stop` makes a failed step exit non-zero; the
/// progress preference silences progress CLIXML; `E` writes the result line
/// with every non-ASCII character as a `\uXXXX` escape.
const PRELUDE: &str = "$ProgressPreference='SilentlyContinue';$ErrorActionPreference='Stop';function E($o){$j=ConvertTo-Json -InputObject $o -Compress -Depth 6;$j=[regex]::Replace($j,'[^\\x00-\\x7e]',{param($m)'\\u{0:x4}' -f [int][char]$m.Value});[Console]::Out.WriteLine('IGN-RESULT:'+$j)};";

/// `Move-Item -Force`: replace `dst` with `src`. A missing source or an
/// unwritable target is a terminating error (exit 1).
///
/// This is the apply-era fallback and is no longer the release-record replace
/// path: on Windows PowerShell 5.1 `Move-Item -Force` over an existing file is
/// delete-then-rename, so a concurrent reader sees the target missing for a
/// moment (G-25-4). The record uses [`RECORD_REPLACE`]. It is kept for the
/// generic exec test and as the negative control of the live race gate.
pub const MOVE_REPLACE: &str = "Move-Item -LiteralPath $p.src -Destination $p.dst -Force;E @{ok=1}";

/// The atomic release-record replace (25-20, G-25-4): `MoveFileEx(src, dst,
/// MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` declared through
/// `Add-Type` P/Invoke. Win32 performs the rename-with-replace as one
/// operation, so a concurrent reader of `dst` sees the old file or the new one
/// on NTFS, never a missing or a half-written one. That is the guarantee of
/// the atomic rename of NTFS on Windows (and of `rename(2)` on Linux, which the
/// POSIX arm uses): it is atomic for readers and it is not a durable write
/// (the temporary file is not flushed first). `Move-Item -Force` was retired
/// for the record because on PowerShell 5.1 it deletes the target before the
/// rename.
///
/// It fails closed: unless the session is `FullLanguage` the script throws
/// naming the language mode, and a failing `Add-Type` throws naming the
/// innermost exception, so the writer returns `transport_failed` and never
/// falls back to a non-atomic replace. The type is compiled only when
/// `IgnRecord.Native` is not loaded yet (production runs one fresh process per
/// replace, so this matters only to the in-process race harness). A sharing or
/// access violation (Win32 error 32 or 5, a reader holding the target without
/// delete-share) is retried `$p.retries` times with one-second pauses, then the
/// script throws naming the last error and its system message. Error 5 also
/// covers a permanent ACL denial, which therefore surfaces after about
/// `retries` seconds. A replace still blocked after the retries throws and the
/// previous record stays. `$p.src`, `$p.dst` and `$p.retries` are data in the
/// base64 parameter blob, never text of the script.
///
/// Every failure says why (G-25-5, WR-01). The variable part of a thrown text
/// is one line of printable ASCII, at most 200 characters plus `...` when cut,
/// with every drive-letter or UNC path replaced by `<path>`. It is only
/// concatenated into the thrown string, never evaluated. Each thrown text
/// stays within 300 characters: PowerShell 5.1 wraps error lines at 119
/// columns and a thrown string ends its error record with
/// `FullyQualifiedErrorId : <thrown text>`, so at most 3 such chunks and the
/// CategoryInfo line must fit the 5-line stderr tail kept by `scrub_stderr`.
/// The helpers are named `IgnLine` and `IgnCause` and never with one letter,
/// because PowerShell resolves an alias before a function (`r` is the built-in
/// alias of `Invoke-History`). `IgnCause` has its own try and catch, so a
/// failure inside it can never stand in for the original failure.
///
/// `Add-Type` is kept (the G-25-5 evaluation): a `DefinePInvokeMethod`
/// declaration needs a dynamic assembly, a module, a type and a custom
/// attribute that sets `SetLastError`, none of it can be exercised offline,
/// and in-memory P/Invoke through Reflection.Emit is the technique of
/// offensive tooling that endpoint protection watches. The cost is one
/// `csc.exe` compile in every fresh process, inside a whole fresh-process
/// replace of 2.70 s on the capture host (25-20 preflight). The prerequisites
/// are a FullLanguage session, `csc.exe` and a writable `%TEMP%` for the
/// deploy account.
pub const RECORD_REPLACE: &str = concat!(
    r#"if($ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage'){throw ('the record replace needs a FullLanguage PowerShell session, not '+$ExecutionContext.SessionState.LanguageMode)};"#,
    r#"function IgnLine($t){$s=[regex]::Replace([string]$t,'[\x00-\x1f]+',' ');$s=[regex]::Replace($s,'(?:[A-Za-z]:[\\/]|\\\\)[^''"]*','<path>');$s=[regex]::Replace($s,'[^\x20-\x7e]','?').Trim();if($s.Length -gt 200){$s=$s.Substring(0,200)+'...'};$s};"#,
    r#"function IgnCause($x){try{while($x.InnerException){$x=$x.InnerException};$m=[string]$x.Message;$k=$x.GetType().FullName+' '+('0x{0:X8}' -f $x.HResult);if($m){$k+=': '+$m};IgnLine $k}catch{'the cause could not be read'}};"#,
    "if(-not ('IgnRecord.Native' -as [type])){try{Add-Type -Namespace IgnRecord -Name Native -MemberDefinition '[DllImport(\"kernel32.dll\",SetLastError=true,CharSet=CharSet.Unicode)]public static extern bool MoveFileEx(string a,string b,uint c);'}catch{throw ('the record replace cannot declare MoveFileEx: '+(IgnCause $_.Exception))}};",
    "$f=[uint32](0x1 -bor 0x8);$n=0;",
    "while(-not [IgnRecord.Native]::MoveFileEx($p.src,$p.dst,$f)){$c=[Runtime.InteropServices.Marshal]::GetLastWin32Error();",
    r#"if(($c -ne 32 -and $c -ne 5) -or $n -ge [int]$p.retries){throw ('the record replace failed with Win32 error '+$c+': '+(IgnLine $(try{(New-Object ComponentModel.Win32Exception ([int]$c)).Message}catch{''})))};"#,
    "$n++;Start-Sleep -Seconds 1};E @{ok=1}"
);

/// Walk each `$p.r[i]` (with the prefix `$p.x` in front) and report every
/// entry as `[relative path, is_dir, size, is_reparse_point, absolute path
/// length]`. A reparse point is reported and not descended. A missing root is
/// `x=0`. An unreadable directory is a terminating error.
///
/// The relative path is built from entry names during the walk (`\`-joined),
/// never by cutting the root off `FullName`: .NET rewrites a root it
/// normalises (an 8.3 component such as `RUNNER~1` expanded, `.`, a doubled or
/// trailing separator dropped), so the caller's spelling and `FullName` need
/// not share a prefix length (G-22-1).
pub const ENUMERATE: &str = "$o=@();for($i=0;$i -lt $p.r.Count;$i++){$b=$p.x+$p.r[$i];$e=@();$x=[IO.Directory]::Exists($b);if($x){$s=New-Object Collections.Stack;$s.Push(@([IO.DirectoryInfo]$b,''));while($s.Count){$q=$s.Pop();foreach($f in $q[0].EnumerateFileSystemInfos()){$a=[int]$f.Attributes;$d=[int](($a -band 16) -ne 0);$r=[int](($a -band 1024) -ne 0);$z=0;if(!$d){$z=$f.Length};$m=$q[1]+$f.Name;$e+=,@($m,$d,$z,$r,$f.FullName.Length);if($d -and !$r){$s.Push(@($f,($m+'\\')))}}}};$o+=@{i=$i;x=[int]$x;e=$e}};E @{ok=1;res=$o}";

/// List the direct children of each `$p.r[i]` (with the prefix `$p.x` in
/// front), with the columns of [`ENUMERATE`]: `[name, is_dir, size,
/// is_reparse_point, absolute path length]`. No child is opened, so nothing
/// below a child directory is read (C6, PR #31 review: the names-only
/// `tag-provider` cross-check must not walk a provider's subtree). A missing
/// root is `x=0`. An unreadable root is a terminating error.
pub const ENUMERATE_CHILDREN: &str = "$o=@();for($i=0;$i -lt $p.r.Count;$i++){$b=$p.x+$p.r[$i];$e=@();$x=[IO.Directory]::Exists($b);if($x){foreach($f in ([IO.DirectoryInfo]$b).EnumerateFileSystemInfos()){$a=[int]$f.Attributes;$d=[int](($a -band 16) -ne 0);$r=[int](($a -band 1024) -ne 0);$z=0;if(!$d){$z=$f.Length};$e+=,@($f.Name,$d,$z,$r,$f.FullName.Length)}};$o+=@{i=$i;x=[int]$x;e=$e}};E @{ok=1;res=$o}";

/// Zip exactly the files of an uploaded read list into one archive (quick
/// 261105-dl1), so a Windows read moves its files with one sftp `get`
/// instead of one `get` per file (about 0.37 s each on this server).
///
/// Parameters (`$p`): `x` the long path prefix, `b` the native directory the
/// listed names are below (`<data>\config\resources` for a resource read,
/// `<data>\projects\<p>\.git` for the object read of 27-11), `l` the native
/// list path, `z` the native archive path, `h` the lowercase hex SHA-256 of
/// the list bytes, `f` the free bytes the archive's drive must have, and
/// optionally `c`: `NoCompression` stores the entries (git objects and packs
/// are compressed already); anything else, or no `c`, is `Optimal`, the level
/// every resource read has always used.
///
/// - Reads the list (`{size}` TAB `{collection}/{rel}` LF, see
///   `bundle::render_list`) and refuses it unless its SHA-256 is `h`
///   (`{ok:0,err:'list'}`): the names come from the list ign uploaded, never
///   from a walk on the host.
/// - Refuses when the drive of `z` has fewer than `f` bytes free
///   (`{ok:0,err:'space',free}`).
/// - Creates `z` with `CreateNew` (an existing file there is never written
///   or removed: the call fails) and wraps it in a `ZipArchive` in Create
///   mode. Per line it opens the source below `b` with `FileShare`
///   `ReadWrite|Delete` (a gateway writing or replacing the file is not
///   blocked), makes an entry named `{collection}/{rel}` with Deflate
///   (`Optimal`, or `NoCompression` when `c` says so), copies, and compares the bytes copied with the listed size.
///   Each read asks for at most the bytes still allowed, so the copy stops
///   one byte past the listed size: a file that keeps growing cannot grow
///   the archive (quick 261105-cr1). A missing source is
///   `{ok:0,err:'missing',i}`, a size change `{ok:0,err:'changed',i,n}` with
///   `n` the bytes copied (the listed size plus one when the file is
///   larger).
/// - Answers `{ok:1,n,z}`: the files bundled and the archive bytes.
/// - Its `finally` removes the list always, and removes the archive unless
///   the answer is `ok:1` and this call created it.
///
/// The list is split with `Split(char[], StringSplitOptions)`: .NET
/// Framework has no `Split(char, StringSplitOptions)`, so on Windows
/// PowerShell 5.1 that call binds to `Split(params char[])`, keeps the empty
/// entry after the last LF and fails on it (found live, quick 261105-dl1;
/// PowerShell 7 has both overloads).
///
/// `ZipArchive` (`System.IO.Compression`, .NET Framework 4.5+, in-box in
/// PowerShell 7) is used, not `ZipFile` (a second assembly) and not
/// `tar.exe` (absent on Server 2016, so it would need a probe and a per-file
/// fallback, the path that does not scale). It is an in-process API, so every
/// path stays data and no external program's arguments are assembled.
/// `Add-Type -AssemblyName` loads the GAC assembly; nothing is compiled.
pub const BUNDLE: &str = concat!(
    "Add-Type -AssemblyName System.IO.Compression;",
    "$x=$p.x;$c=[IO.Path]::DirectorySeparatorChar;$r=$null;$s=$null;$a=$null;$k=$false;",
    "$d=[IO.Compression.CompressionLevel]::Optimal;if($p.c -eq 'NoCompression'){$d=[IO.Compression.CompressionLevel]::NoCompression};",
    "try{$t=[IO.File]::ReadAllBytes($x+$p.l);",
    "$h=[BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($t)).Replace('-','').ToLowerInvariant();",
    "if($h -ne $p.h){$r=@{ok=0;err='list'}}else{",
    "$v=(New-Object IO.DriveInfo([IO.Path]::GetPathRoot($p.z))).AvailableFreeSpace;",
    "if($v -lt [int64]$p.f){$r=@{ok=0;err='space';free=$v}}else{",
    "$w=[Text.Encoding]::UTF8.GetString($t).Split([char[]]@([char]10),[StringSplitOptions]::RemoveEmptyEntries);",
    "$s=[IO.File]::Open($x+$p.z,'CreateNew','ReadWrite','None');$k=$true;",
    "$a=New-Object IO.Compression.ZipArchive($s,[IO.Compression.ZipArchiveMode]::Create);",
    "$u=New-Object byte[] 65536;",
    "for($i=0;$i -lt $w.Count;$i++){$j=$w[$i].IndexOf([char]9);$n=[int64]$w[$i].Substring(0,$j);$m=$w[$i].Substring($j+1);",
    "$q=$x+$p.b+$c+$m.Replace('/',[string]$c);",
    "if(-not [IO.File]::Exists($q)){$r=@{ok=0;err='missing';i=$i};break};",
    "$g=[int64]0;$in=[IO.File]::Open($q,'Open','Read','ReadWrite,Delete');",
    "try{$o=$a.CreateEntry($m,$d).Open();",
    "try{while($g -le $n){$e=$n-$g+1;if($e -gt $u.Length){$e=$u.Length};$y=$in.Read($u,0,[int]$e);if($y -le 0){break};$o.Write($u,0,$y);$g+=$y}}finally{$o.Dispose()}}finally{$in.Dispose()};",
    "if($g -ne $n){$r=@{ok=0;err='changed';i=$i;n=$g};break}};",
    "$a.Dispose();$a=$null;$s.Dispose();$s=$null;",
    "if($null -eq $r){$r=@{ok=1;n=$w.Count;z=(New-Object IO.FileInfo($x+$p.z)).Length}}}}}",
    "finally{if($a){try{$a.Dispose()}catch{}};if($s){try{$s.Dispose()}catch{}};",
    "[IO.File]::Delete($x+$p.l);",
    "if($k -and ($null -eq $r -or $r.ok -ne 1)){[IO.File]::Delete($x+$p.z)}};",
    "E $r"
);

/// Remove what a read left in its staging dir (quick 261105-dl1), best
/// effort after a failed bundled read.
///
/// Parameters (`$p`): `x` the long path prefix, `d` the native staging dir,
/// `f` the file names to remove from it (`read.tsv`, `read.zip`).
///
/// - A missing `d` is `{ok:1}`: nothing is left.
/// - A `d` that is a reparse point is `{ok:0,err:'reparse'}` and nothing is
///   touched or followed.
/// - Otherwise each named entry of `d` that is a regular file (not a reparse
///   point) is removed, then `d` when it is empty (`{ok:1}`). When `d` still
///   holds anything else the answer is `{ok:0,err:'left'}` and nothing else
///   is touched.
pub const BUNDLE_CLEAN: &str = concat!(
    "$c=[IO.Path]::DirectorySeparatorChar;$d=$p.x+$p.d;",
    "if(-not [IO.Directory]::Exists($d)){E @{ok=1}}else{",
    "$i=New-Object IO.DirectoryInfo($d);",
    "if(([int]$i.Attributes -band 1024) -ne 0){E @{ok=0;err='reparse'}}else{",
    "foreach($n in $p.f){$q=$d+$c+$n;if([IO.File]::Exists($q)){",
    "if(([int](New-Object IO.FileInfo($q)).Attributes -band 1024) -eq 0){[IO.File]::Delete($q)}}};",
    "if(@($i.EnumerateFileSystemInfos()).Count -gt 0){E @{ok=0;err='left'}}else{[IO.Directory]::Delete($d);E @{ok=1}}}}"
);

/// The `.git` probe of `deploy projects` (Phase 25, 25-08, K9): what is at
/// `$p.c` (`<data>\projects\<name>\.git`), asked only when `$p.a`
/// (`<data>\projects`) and `$p.b` (the project directory) are real
/// directories.
///
/// Each level is one `Get-Item -LiteralPath ... -Force` and nothing else. The
/// 25-03 captures (`## C-H before grant`, `## Host grant and prod git.yaml
/// (25-03)`) showed why: `-Force` sees a Hidden entry, the .NET `Exists`
/// methods and `Test-Path` answer false (or, run to run, throw) for a
/// directory the account cannot traverse, and `Get-Item` throws
/// `UnauthorizedAccessException` there. Only an `ObjectNotFound` error record
/// is `none`; every other error is rethrown, which under the prelude's `Stop`
/// is a non-zero exit and so `transport_failed`, never "absent". A reparse
/// point (junction or symlink) is reported with `r=1` and never opened or
/// followed; a Hidden `.git` is reported with `h=1` and counts like any other.
///
/// Result: `projects` and `project` are `none | dir | file | symlink | skip`,
/// `kind` is `none | dir | file | skip` for the `.git` entry, `hidden` and
/// `reparse` are 0 or 1 for it.
pub const GIT_ENTRY: &str = concat!(
    "function T($x){try{$i=Get-Item -LiteralPath $x -Force -ErrorAction Stop}",
    "catch{if($_.CategoryInfo.Category -eq 'ObjectNotFound'){return @{k='none';h=0;r=0}};throw};",
    "$a=[int]$i.Attributes;",
    "return @{k=$(if($a -band 16){'dir'}else{'file'});h=[int](($a -band 2) -ne 0);r=[int](($a -band 1024) -ne 0)}};",
    "function W($t){if($t.r){'symlink'}else{$t.k}};",
    "$s=@{k='skip';h=0;r=0};$a=T $p.a;$b=$s;$c=$s;",
    "if($a.k -eq 'dir' -and !$a.r){$b=T $p.b;if($b.k -eq 'dir' -and !$b.r){$c=T $p.c}};",
    "E @{ok=1;projects=(W $a);project=(W $b);kind=$c.k;hidden=$c.h;reparse=$c.r}"
);

// Phase 27 (27-09): the Windows half of the git host channel. Each script is
// one `const` built from shared pieces so the text a test runs is the text a
// host receives; every path travels in `$p`.

// ---- generated pieces (see the GIT_* docs) ----
macro_rules! git_ps_base {
    () => {
        r##"$c=[IO.Path]::DirectorySeparatorChar;function Z($x){try{Get-Item -LiteralPath $x -Force}catch{if($_.CategoryInfo.Category -ne 'ObjectNotFound'){throw}}};function K($i){if(!$i -or !$i.PSIsContainer -or ([int]$i.Attributes -band 1024)){throw 'a path is not a real directory'};$i};"##
    };
}
macro_rules! git_ps_yc {
    () => {
        r##"function Yc($s,$d){& robocopy $s $d /E /COPY:DAT /R:1 /W:1 /NFL /NDL /NJH /NJS /NP /XJ|Out-Null;if($LASTEXITCODE -ge 8){throw "robocopy failed with exit $LASTEXITCODE"}};"##
    };
}
macro_rules! git_ps_x {
    () => {
        r##"function X($b){[BitConverter]::ToString($b).Replace('-','').ToLower()};"##
    };
}
macro_rules! git_ps_m {
    () => {
        r##"function X($b){[BitConverter]::ToString($b).Replace('-','').ToLower()};function M($d){$d=$p.x+$d;$h=[Security.Cryptography.SHA256]::Create();$n=0;$b=0;$l=@();$s=New-Object Collections.Stack;$s.Push(@([IO.DirectoryInfo]$d,''));while($s.Count){$q=$s.Pop();foreach($f in $q[0].GetFileSystemInfos()){$a=[int]$f.Attributes;if(($a -band 1024) -or $f.Name -match '[\x00-\x1f]'){throw 'the tree holds a link or a name with a control character'};$r=$q[1]+$f.Name;if($a -band 16){$s.Push(@($f,($r+'/')))}else{$n++;$b+=$f.Length;if($r.StartsWith('objects/')){$v=$f.Length}else{$m=[IO.File]::Open($f.FullName,'Open','Read','ReadWrite,Delete');$v=X $h.ComputeHash($m);$m.Dispose()};$l+=$r+"`0"+$v}}};[Array]::Sort($l,[StringComparer]::Ordinal);@{ok=1;files=$n;bytes=$b;digest=(X $h.ComputeHash([Text.Encoding]::UTF8.GetBytes($(if($l.Length){($l-join"`n")+"`n"}else{''}))))}}"##
    };
}
macro_rules! git_ps_dr {
    () => {
        r##"function Dy($i){foreach($f in $i.GetFileSystemInfos()){$a=[int]$f.Attributes;if($a -band 1024){}elseif($a -band 16){Dy $f}elseif($a -band 1 -and !(Z $f.FullName).LinkType){$f.Attributes=128}}};function Dr($d){$null=K (Z $d);$d=$p.x+$d;Dy ([IO.DirectoryInfo]$d);[IO.Directory]::Delete($d,$true)};"##
    };
}
macro_rules! git_ps_l {
    () => {
        r##"function L($t){$g+$c+($t+$(if($t.StartsWith('objects/pack/')){'.ign-tmp'}else{'.lock'})).Replace('/',$c)};"##
    };
}

macro_rules! git_ps_w {
    () => {
        r##"function G(){$q=$p.d+$c+'projects';$null=K (Z $q);$q+=$c+$p.j;$null=K (Z $q);$q+=$c+'.git';$null=K (Z $q);$q};$g=G;$u=$p.h+$c+'ign-deploy';function Pc($t,$m){$q=G;$x=$t.Split('/');for($k=0;$k -lt $x.Length-1;$k++){$q+=$c+$x[$k];$y=Z $q;if(!$y -and $m){$null=[IO.Directory]::CreateDirectory($q);$y=Z $q};$null=K $y}};"##
    };
}
macro_rules! git_ps_sn {
    () => {
        r##"if($p.s -notmatch '^git-stage-[A-Za-z0-9._-]+$'){throw 'refusing the stage name'};"##
    };
}
macro_rules! git_ps_sx {
    () => {
        r##"function Sx(){foreach($f in ([IO.DirectoryInfo]$u).GetFileSystemInfos()){if(!([int]$f.Attributes -band 1040)){$f.Delete()}};[IO.Directory]::Delete($u)};"##
    };
}

/// The fixed files of a gateway's `.git` in one pass (Phase 27, 27-09), the
/// Windows half of the POSIX `GIT_STATE_SH`: `HEAD`, `config`, `packed-refs`,
/// the tail of `logs/HEAD`, whether `index` exists, the markers of
/// [`GIT_MARKERS`](crate::deploy::git_state::GIT_MARKERS) that are present and
/// every loose ref below `refs/heads` and `refs/remotes`.
///
/// Parameters (`$p`): `a` the native `.git` path, `m` the marker names. Each
/// file is read with `FileShare.ReadWrite|Delete` (a running gateway is not
/// blocked) and answered as `{n:size,b:base64}` or `null` when absent; a file
/// is read up to one byte past its cap, so the caller's cap check names an
/// oversized one. A reparse point, a directory where a file belongs or a name
/// with a control character is a terminating error, never an absent file:
/// presence is decided by `Get-Item -Force`, which throws unless the error is
/// `ObjectNotFound` (the 25-03 captures). Every directory above a file and
/// above each loose-ref root (`logs`, `refs`) must be a real directory too,
/// checked before anything below it is touched (CodeRabbit #56: `Get-Item` on
/// the final path does not see a junction above it); an absent one is an
/// absent file. Loose refs stop at 4097 entries.
pub const GIT_STATE: &str = concat!(
    git_ps_base!(),
    r##"$g=$p.a;K (Z $g)|Out-Null;function P($f){$x=$f.Split('/');$h=$g;for($i=0;$i -lt $x.Length-1;$i++){$h+=$c+$x[$i];$j=Z $h;if(!$j){return $false};$null=K $j};$true};function F($f,$k,$t){if(!(P $f)){return $null};$q=$g+$c+$f.Replace('/',$c);$y=Z $q;if(!$y){return $null};if($y.PSIsContainer -or (([int]$y.Attributes -band 1024) -ne 0)){throw "$f is not a regular file"};$n=$y.Length;$e=[int][Math]::Min([long]$n,[long]$k);$u=New-Object byte[] $e;$s=[IO.File]::Open($q,'Open','Read','ReadWrite,Delete');try{if($t -and $n -gt $e){[void]$s.Seek($n-$e,0)};$o=0;while($o -lt $e){$w=$s.Read($u,$o,$e-$o);if($w -le 0){break};$o+=$w}}finally{$s.Dispose()};@{n=$n;b=[Convert]::ToBase64String($u,0,$o)}};$k=@();foreach($m in @($p.m)){if(Z ($g+$c+$m.Replace('/',$c))){$k+=$m}};$u=@();$z=0;foreach($d in 'refs/heads','refs/remotes'){$y=$null;if(P $d){$y=Z ($g+$c+$d.Replace('/',$c))};if($y){K $y|Out-Null;$s=New-Object Collections.Stack;$s.Push(@($y,$d));while($s.Count -and $z -le 4097){$q=$s.Pop();foreach($f in $q[0].EnumerateFileSystemInfos()){$a=[int]$f.Attributes;if((($a -band 1024) -ne 0) -or $f.Name -match '[\x00-\x1f]'){throw 'refs holds a link or a name with a control character'};$r=$q[1]+'/'+$f.Name;if($a -band 16){$s.Push(@($f,$r))}else{$z++;if($z -le 4097){$v=F $r 65537 $false;$v.p=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($r));$u+=,$v}}}}}};E @{ok=1;head=(F 'HEAD' 65537 $false);config=(F 'config' 65537 $false);packed=(F 'packed-refs' 8388609 $false);log=(F 'logs/HEAD' 65536 $true);index=[bool](Z ($g+$c+'index'));markers=$k;refs=$u}"##,
);

/// Save a project's `.git` below the account home with ONE host-local copy
/// (Phase 27, D-06): `robocopy /E /COPY:DAT /R:1 /W:1 /XJ`, the tool G4
/// measured at 1.4 s for the production-shape `.git` (the same files as
/// `Copy-Item` in a third of the restore time, and it carries attributes).
///
/// Parameters: `x` the long path prefix, `d` the native data directory, `j` the
/// project, `h` the account home, `g` the gateway, `i` the run id; the paths
/// are built in the script from those components so a long project name costs
/// the command budget once. The script takes the manifest of the source first
/// (it refuses a link or a name with a control character anywhere in the
/// tree), walks `ign-deploy\<g>\projects\<j>\git-save\<i>` below the home
/// (a reparse point on the way is refused, missing levels are created), makes
/// `repo` with `New-Item`, which fails when anything is there (a save is never
/// overwritten and a planted link is never written through), copies, carries
/// the source's Hidden attribute to `repo` (robocopy copies the contents of a
/// directory, not the directory's own attributes) and compares the manifest of
/// the copy with the source's. A failure removes the partial `repo`
/// (`Dr`, below). The answer is the copy's manifest.
///
/// WR-B5 (27-33, confirmed on the Windows test VM under 5.1): robocopy
/// `/COPY:DAT` keeps the ReadOnly attribute of git's pack and object files, and
/// `[IO.Directory]::Delete(path, $true)` stops on such a file with `Access to the
/// path ... is denied`, which in a `catch` replaced the real error. Every
/// recursive delete of this family goes through `Dr`: it walks the tree with
/// `EnumerateFileSystemInfos`, never enters or changes a reparse point (a link
/// is left for `Directory.Delete`, which removes the link itself), sets the
/// attributes of each ReadOnly file to Normal (the file is about to be deleted),
/// then deletes. A delete that still fails does not hide the original error:
/// the script rethrows it with the leftover path appended.
pub const GIT_SAVE: &str = concat!(
    git_ps_base!(),
    git_ps_yc!(),
    git_ps_m!(),
    git_ps_dr!(),
    r##"$a=$p.d+$c+'projects'+$c+$p.j+$c+'.git';$i=K (Z $a);$m=M $a;$q=$p.h;foreach($e in 'ign-deploy',$p.g,'projects',$p.j,'git-save',$p.i){$q+=$c+$e;$y=Z $q;if($y){$null=K $y}else{$null=[IO.Directory]::CreateDirectory($q)}};$r=$q+$c+'repo';$null=ni -Type Directory $r;try{Yc $a $r;if([int]$i.Attributes -band 2){($h=Z $r).Attributes=$h.Attributes -bor 2};$n=M $r;if($n.digest -ne $m.digest){throw 'the saved copy does not match the .git it was taken from'}}catch{$e=$_;try{Dr $r}catch{$e=$e.Exception.Message+' (the partial copy at '+$r+' could not be removed)'};throw $e};E $n"##,
);

/// Restore the save of a run as the project's `.git` (Phase 27, D-06):
/// the staged copy, the manifest compare and one rename.
///
/// Parameters as [`GIT_SAVE`] (`d` is the data directory, `j` the project). It
/// refuses when `.git` exists (it is never replaced), checks the project
/// directory and every level of the save path are real directories, takes the
/// manifest of the save, creates the stage `<project>\.ign-restore-<run>` with
/// `New-Item` (which fails when anything is there), copies the save into it
/// with robocopy, re-applies the Hidden attribute the save recorded (G8: the
/// module's `.git` is Hidden; the stage is a new directory and robocopy would
/// otherwise leave it visible), proves the stage equals the save and renames it
/// to `.git` with `Directory.Move`, which fails when the target exists. The
/// stage inherits the project directory's ACL, so the LocalSystem gateway
/// account holds the FullControl G1 found. The answer is the manifest of the
/// restored `.git`; a failure removes the stage.
pub const GIT_RESTORE: &str = concat!(
    git_ps_base!(),
    git_ps_yc!(),
    git_ps_m!(),
    git_ps_dr!(),
    r##"$a=$p.d+$c+'projects'+$c+$p.j;$null=K (Z $a);$g=$a+$c+'.git';if(Z $g){throw '.git exists; never replaced'};$q=$p.h;foreach($e in 'ign-deploy',$p.g,'projects',$p.j,'git-save',$p.i,'repo'){$q+=$c+$e;$i=K (Z $q)};$m=M $q;$t=$a+$c+'.ign-restore-'+$p.i;$null=ni -Type Directory $t;try{Yc $q $t;if([int]$i.Attributes -band 2){($h=Z $t).Attributes=$h.Attributes -bor 2};$n=M $t;if($n.digest -ne $m.digest){throw 'the copy differs'};[IO.Directory]::Move($t,$g)}catch{$e=$_;try{Dr $t}catch{$e=$e.Exception.Message+' (the partial copy at '+$t+' could not be removed)'};throw $e};E (M $g)"##,
);

/// The manifest of a tree: file count, total bytes and one digest (Phase 27).
///
/// Parameters: `x` the long path prefix, `a` the native directory (a project's
/// `.git` or a save's `repo`). The digest is the SHA-256 over the lines `path
/// NUL size LF` for every file below `objects/` and `path NUL sha256 LF` for
/// every other file, sorted by the ordinal order of the whole path (the order
/// `LC_ALL=C sort` and the local walk give for the ASCII names a `.git` holds),
/// with `/` separators, so the three host families compute one value. The
/// relative path is built from entry names during the walk (as in
/// [`ENUMERATE`]); a reparse point or a name with a control character
/// anywhere in the tree is a terminating error and nothing is followed; the
/// root itself must be a real directory, so a junction at `a` is refused
/// rather than walked as its target. Files are hashed through a
/// `FileShare.ReadWrite|Delete` stream.
pub const GIT_MANIFEST: &str = concat!(
    git_ps_base!(),
    git_ps_m!(),
    r##"$null=K (Z $p.a);E (M $p.a)"##,
);

/// How big a `.git` is, from one enumeration (Phase 27, checker W-5):
/// file count and the sum of the file lengths. Parameters: `x`, `a` as
/// [`GIT_MANIFEST`]. The directory listing carries each length, so no file is
/// opened; a reparse point or a name with a control character is refused.
pub const GIT_SIZE: &str = concat!(
    git_ps_base!(),
    r##"K (Z $p.a)|Out-Null;$s=New-Object Collections.Stack;$s.Push([IO.DirectoryInfo]($p.x+$p.a));$n=0;$b=[long]0;while($s.Count){foreach($f in $s.Pop().EnumerateFileSystemInfos()){$a=[int]$f.Attributes;if((($a -band 1024) -ne 0) -or $f.Name -match '[\x00-\x1f]'){throw 'the tree holds a link or a name with a control character'};if($a -band 16){$s.Push($f)}else{$n++;$b+=$f.Length}}};E @{ok=1;files=$n;bytes=$b}"##,
);

/// The object files of a gateway's `.git` in one enumeration (Phase 27, 27-11,
/// D-24): the Windows half of the POSIX `GIT_OBJECTS_SH`.
///
/// Parameters: `x` the long path prefix, `a` the native `.git`, `l` the most
/// entries it may walk (`GIT_OBJECTS_LIST_MAX_ENTRIES`), `m` the most it may
/// report (`GIT_OBJECTS_LOOSE_MAX + 2 * GIT_OBJECTS_PACKS_MAX + 1`). `.git` and
/// `objects` must be real directories; the walk goes two levels below
/// `objects` (`objects/<dir>/<file>`) and nothing else is opened. A reparse
/// point at either level, or a name with a control character, a backslash or a
/// colon, is a terminating error, and so is a walk past `l` entries. Only the
/// files of the fixed shapes (`<2 hex>/<38 hex>`, `pack/pack-<40 hex>.pack` and
/// `.idx`, lower case) are reported, as the lines `size SP objects/<name>` of
/// one string `t`, so a large listing is one string for
/// `ConvertTo-Json` and the walk appends to a list, never to an array. Answers
/// `{ok:1,n:<lines>,more:<0|1>,t:<lines joined by LF>}`; `more` is 1 when the
/// report stopped at `m` lines, and the caller refuses such a listing.
pub const GIT_OBJECTS: &str = concat!(
    git_ps_base!(),
    r##"$g=$p.a;K (Z $g)|Out-Null;$o=$g+$c+'objects';K (Z $o)|Out-Null;$l=New-Object Collections.ArrayList;$k=0;$u=0;function V($f){if(([int]$f.Attributes -band 1024) -ne 0 -or $f.Name -match '[\x00-\x1f\\:]'){throw 'objects holds a link or a name that is not a plain name'};$global:k++;if($global:k -gt $p.l){throw 'objects holds more entries than a read may walk'}};foreach($f in ([IO.DirectoryInfo]($p.x+$o)).EnumerateFileSystemInfos()){V $f;if([int]$f.Attributes -band 16){foreach($h in $f.EnumerateFileSystemInfos()){V $h;if(([int]$h.Attributes -band 16) -eq 0){$n=$f.Name+'/'+$h.Name;if($n -cmatch '^([0-9a-f]{2}/[0-9a-f]{38}|pack/pack-[0-9a-f]{40}\.(pack|idx))$'){if($l.Count -ge $p.m){$u=1}else{[void]$l.Add([string]$h.Length+' objects/'+$n)}}}}}};E @{ok=1;n=$l.Count;more=$u;t=($l -join "`n")}"##,
);

/// The SHA-256 of one file below a `.git` (Phase 27, the read-back of an
/// advance's writes). Parameters: `a` the native `.git`, `f` the `/`-separated
/// path below it. Every directory on the way must be a real directory; a
/// missing level or file answers `digest:null`; a link or a directory in the
/// file's place is refused.
pub const GIT_DIGEST: &str = concat!(
    git_ps_base!(),
    git_ps_x!(),
    r##"K (Z $p.a)|Out-Null;$q=$p.a;$o=0;$u=$p.f.Split('/');for($k=0;$k -lt $u.Length;$k++){$q+=$c+$u[$k];$y=Z $q;if(!$y){$o=1;break};if($k -lt $u.Length-1){K $y|Out-Null}elseif($y.PSIsContainer -or (([int]$y.Attributes -band 1024) -ne 0)){throw 'the file is not a regular file'}};if($o){E @{ok=1;digest=$null}}else{$m=[IO.File]::Open($q,'Open','Read','ReadWrite,Delete');try{$d=X ([Security.Cryptography.SHA256]::Create().ComputeHash($m))}finally{$m.Dispose()};E @{ok=1;digest=$d}}"##,
);

/// The private stage of a `.git` write (Phase 27, 27-33, WR-B3): `ign-deploy\<stage>`
/// in the deploy account's home, where sftp puts the bytes of the write. Parameters:
/// `h` the native account home, `s` the stage name (`git-stage-<id>`, one component).
/// `ign-deploy` is made when absent and refused when it is a link; the stage is made
/// with `New-Item`, which fails when anything is there, so a planted entry is never
/// reused and never removed.
pub const GIT_STAGE: &str = concat!(
    git_ps_base!(),
    r##"if($p.s -notmatch '^git-stage-[A-Za-z0-9._-]+$'){throw 'refusing the stage name'};$u=$p.h+$c+'ign-deploy';$y=Z $u;if($y){$null=K $y}else{$null=[IO.Directory]::CreateDirectory($u)};$null=ni -Type Directory ($u+$c+$p.s);E @{ok=1}"##,
);

/// Create each lock of a `.git` write exclusively and fill it from the stage
/// (Phase 27, 27-33, WR-B3), the Windows half of the POSIX `GIT_WRITE_SH`.
///
/// Parameters: `d` the native data directory, `j` the project, `h` the account
/// home, `s` the stage name, `o` the plan index of the first entry, `w` entries
/// `<path>:<size>` (the paths below `.git`; the lock of a path below
/// `objects/pack/` is `<path>.ign-tmp`, any other `<path>.lock`). The bytes of
/// plan index `o + n` are in `<home>\ign-deploy\<stage>\<o + n>`.
///
/// Nothing is written through a path the gateway account can change. The walk
/// from `projects` down to the lock's directory checks every level (`K`: a real
/// directory, not a reparse point); a missing level is made one directory at a
/// time and checked. The target must be absent or a regular file, and the lock
/// must be absent (`Get-Item -Force` also sees a dangling link). The stage file
/// must be a regular file of the size that was sent. The lock is opened with
/// `FileMode.CreateNew` and `FileShare.None`, so nobody can delete, rename or
/// replace it while it is open; before one byte is written the whole walk is
/// checked again and the lock must be a regular, empty file (a link swapped in
/// between would have sent the create elsewhere, and nothing was written). Only
/// then is the stage file copied into the open lock. On any failure the locks
/// this call made are removed and the error is rethrown.
pub const GIT_LOCK: &str = concat!(
    git_ps_base!(),
    git_ps_l!(),
    git_ps_w!(),
    r##"$null=K (Z $u);$u+=$c+$p.s;$null=K (Z $u);$made=@();$s=$null;$n=$null;try{$i=[int]$p.o;foreach($e in @($p.w)){$k=$e.LastIndexOf(':');$t=$e.Substring(0,$k);$z=[long]$e.Substring($k+1);Pc $t 1;$y=Z ($g+$c+$t.Replace('/',$c));if($y -and ($y.PSIsContainer -or ([int]$y.Attributes -band 1024))){throw "$t is not a regular file"};$l=L $t;if(Z $l){throw "$l already exists"};$f=$u+$c+$i;$y=Z $f;if(!$y -or $y.PSIsContainer -or ([int]$y.Attributes -band 1024) -or $y.Length -ne $z){throw "the staged bytes of $t are not a file of the size sent"};$n=[IO.File]::Open($f,'Open','Read','Read');$s=[IO.File]::Open($l,'CreateNew','Write','None');$made+=$l;Pc $t;$y=Z $l;if(!$y -or $y.PSIsContainer -or ([int]$y.Attributes -band 1024) -or $y.Length){throw "$l changed while it was created"};$n.CopyTo($s);$s.Dispose();$s=$null;$n.Dispose();$n=$null;$i++}}catch{$e=$_;if($s){$s.Dispose()};if($n){$n.Dispose()};foreach($l in $made){$y=Z $l;if($y -and !$y.PSIsContainer -and !([int]$y.Attributes -band 1024)){[IO.File]::Delete($l)}};throw $e};E @{ok=1}"##,
);

/// Take away the locks of a failed `.git` write that hold ign's own bytes
/// (Phase 27, 27-33, WR-B2). Parameters as [`GIT_LOCK`], plus `r`: remove the
/// stage afterwards. A lock is removed only when it is a regular file and its
/// length and SHA-256 equal those of the matching stage file; a lock that holds
/// other bytes (a JGit `index.lock`), a lock whose stage file is missing and a
/// link are left alone. The answer is `{ok:1,removed,kept}`. A stage that is
/// missing is not an error. With `r` the stage is removed (its files, then the
/// directory, which fails and names itself when anything else is in it).
pub const GIT_UNLOCK: &str = concat!(
    git_ps_base!(),
    git_ps_x!(),
    git_ps_l!(),
    git_ps_w!(),
    git_ps_sn!(),
    git_ps_sx!(),
    r##"$s=$null;$y=Z $u;$u+=$c+$p.s;if($y){$null=K $y;$s=Z $u;if($s){$null=K $s}};function Hs($f){$m=[IO.File]::Open($f,'Open','Read','ReadWrite,Delete');try{X ([Security.Cryptography.SHA256]::Create().ComputeHash($m))}finally{$m.Dispose()}};$r=0;$h=0;$i=[int]$p.o;foreach($e in @($p.w)){$t=$e.Substring(0,$e.LastIndexOf(':'));$l=L $t;$y=Z $l;if($y -and !$y.PSIsContainer -and !([int]$y.Attributes -band 1024)){$f=$u+$c+$i;$v=$null;if($s){$v=Z $f};if($v -and !$v.PSIsContainer -and !([int]$v.Attributes -band 1024) -and $v.Length -eq $y.Length -and (Hs $l) -eq (Hs $f)){[IO.File]::Delete($l);$r++}else{$h++}};$i++};if($p.r -and $s){Sx};E @{ok=1;removed=$r;kept=$h}"##,
);

/// Put each filled lock over its target (Phase 27), after [`GIT_LOCK`], and
/// with `r` remove the stage. Parameters: `d`, `j`, `h`, `s` as [`GIT_LOCK`], `w`
/// entries `<path>:<size>`, `q` the retries, `r` remove the stage at the end.
/// The walk to the target's directory is checked again before each replace. The
/// lock must be a regular file of exactly the size that was sent; the target
/// must be a regular file or absent. An existing target is replaced with
/// `File.Replace` (atomic for readers), a new one is moved in; a sharing
/// violation is retried `q` times a second apart and then fails, leaving the
/// previous file in place. Not an `Add-Type` P/Invoke: nothing is compiled.
pub const GIT_REPLACE: &str = concat!(
    git_ps_base!(),
    git_ps_l!(),
    git_ps_w!(),
    git_ps_sn!(),
    git_ps_sx!(),
    r##"$null=K (Z $u);$u+=$c+$p.s;$null=K (Z $u);foreach($e in @($p.w)){$k=$e.LastIndexOf(':');$t=$e.Substring(0,$k);$z=[long]$e.Substring($k+1);$l=L $t;$d=$g+$c+$t.Replace('/',$c);Pc $t;$y=Z $l;if(!$y -or $y.PSIsContainer -or ([int]$y.Attributes -band 1024)){throw "$t.lock is not a regular file"};if($y.Length -ne $z){throw "$t.lock is not the size that was sent"};$y=Z $d;if($y -and ($y.PSIsContainer -or ([int]$y.Attributes -band 1024))){throw "$t is not a regular file; it is left alone"};for($n=0;;$n++){try{if($y){[IO.File]::Replace($l,$d,[NullString]::Value)}else{[IO.File]::Move($l,$d)};break}catch{if($n -ge $p.q){throw};Start-Sleep 1}}};if($p.r){Sx};E @{ok=1}"##,
);

/// Prove the grant before any other write (Phase 27): create exclusively and
/// delete one probe file in the project directory and in its `.git`. Parameters:
/// `d` the native data directory, `j` the project, `n` the probe file name. The
/// error names which of the two directories refused (`projects/<j>` or
/// `projects/<j>/.git`, as the POSIX script does); a probe file that already
/// exists is refused and left alone.
pub const GIT_PROBE: &str = concat!(
    git_ps_base!(),
    r##"function W($d,$t){K (Z $d)|Out-Null;$f=$d+$c+$p.n;try{$s=[IO.File]::Open($f,'CreateNew','Write','None');$s.Dispose()}catch{throw "a probe file cannot be created in $t"};[IO.File]::Delete($f)};$a=$p.d+$c+'projects'+$c+$p.j;W $a ('projects/'+$p.j);W ($a+$c+'.git') ('projects/'+$p.j+'/.git');E @{ok=1}"##,
);

/// Remove the `repo` tree of one save (Phase 27). Parameters: `x`, `h` the
/// account home, `r` the path below it (`/`-separated), `i` the run id. A path
/// that is not exactly `ign-deploy/<g>/projects/<p>/git-save/<i>/repo` is
/// refused before anything is touched; every level must be a real directory (a
/// missing level answers `removed:0`); the tree goes through `Dr` (ReadOnly
/// cleared first, a reparse point inside never entered or changed, WR-B5), and a
/// delete that still fails names the leftover path. The empty run directory is
/// then removed.
pub const GIT_SAVE_RM: &str = concat!(
    git_ps_base!(),
    git_ps_dr!(),
    r##"$u=$p.r.Split('/');$n=$u.Length;if($n -lt 4 -or $u[0] -ne 'ign-deploy' -or $u[$n-1] -ne 'repo' -or $u[$n-2] -ne $p.i -or $u[$n-3] -ne 'git-save'){throw 'refusing a path that does not end in git-save/<run-id>/repo'};$q=$p.h;K (Z $q)|Out-Null;$o=1;foreach($e in $u){if($e -in '','.','..'){throw 'refusing an empty, . or .. component'};$q+=$c+$e;$y=Z $q;if(!$y){$o=0;break};K $y|Out-Null};if($o){try{Dr $q}catch{throw ($_.Exception.Message+' (the save at '+$q+' could not be removed)')};$v=[IO.Path]::GetDirectoryName($q);if(!@([IO.Directory]::EnumerateFileSystemEntries($v)).Count){[IO.Directory]::Delete($v)}};E @{ok=1;removed=$o}"##,
);

/// Free bytes on the volume that will hold a save (Phase 27). Parameters: `h`
/// the account home, `s` the save directory below it (`/`-separated). The
/// nearest existing ancestor decides the drive.
pub const GIT_FREE: &str = concat!(
    git_ps_base!(),
    r##"$q=$p.h+$c+$p.s.Replace('/',$c);while($q -and !(Z $q)){$q=[IO.Path]::GetDirectoryName($q)};E @{ok=1;free=(New-Object IO.DriveInfo([IO.Path]::GetPathRoot($q))).AvailableFreeSpace}"##,
);

/// Why `name` cannot be ONE component of a path on a Windows host, or `None`.
///
/// This is the Windows half of the path gate. It refuses everything the POSIX
/// gate refuses (empty, `.`, `..`, a separator, NUL and other control
/// characters) and the ways Windows lets a name leave its directory or mean
/// something else: a `:` (a drive letter injected into a component, or an
/// alternate data stream `name:stream`), the characters NTFS forbids
/// (`< > " | ? *`), a trailing dot or space (Windows strips them, so two names
/// would be one entry) and the reserved device names (`CON`, `NUL`, `COM1`,
/// ... with or without an extension). `\\?\` and UNC prefixes cannot pass as a
/// component because they hold separators and a colon.
pub fn windows_component_problem(name: &str) -> Option<String> {
    let quoted = |why: &str| Some(format!("the name {name:?} {why}"));
    if name.is_empty() {
        return quoted("is empty");
    }
    if name == "." || name == ".." {
        return quoted("is `.` or `..`");
    }
    if let Some(bad) = name.chars().find(|c| {
        c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
    }) {
        return Some(format!(
            "the name {name:?} holds {bad:?}, which cannot be part of a Windows path component (separators, drive letters, alternate data streams and reserved characters)"
        ));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return quoted("ends in a dot or a space, which Windows strips");
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    let device = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            upper
                .strip_prefix(prefix)
                .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
        });
    if device {
        return quoted("is a reserved Windows device name");
    }
    None
}

/// Why `path` is not an absolute path in the sftp spelling of a Windows host
/// (`/C:/dir/...`), or `None`: a leading `/`, a drive letter component
/// (`C:`), at least one more component, and every component passing
/// [`windows_component_problem`]. Refuses `C:/...` (home-relative on this
/// server), UNC (`//host/...`), `\\?\` prefixes, `..`, empty components and a
/// second drive letter.
pub fn windows_absolute_path_problem(path: &str) -> Option<String> {
    let Some(rest) = path.strip_prefix('/') else {
        return Some(format!("{path:?} does not start with `/`"));
    };
    let mut parts = rest.split('/');
    let drive = parts.next().unwrap_or_default();
    let drive_ok = matches!(drive.as_bytes(), [letter, b':'] if letter.is_ascii_alphabetic());
    if !drive_ok {
        return Some(format!(
            "{path:?} does not start with a drive letter component such as `/C:/`"
        ));
    }
    let mut below = 0usize;
    for part in parts {
        if let Some(why) = windows_component_problem(part) {
            return Some(format!("{path:?}: {why}"));
        }
        below += 1;
    }
    if below == 0 {
        return Some(format!("{path:?} is a drive root"));
    }
    None
}

/// One fixed script and its parameters.
#[derive(Debug, Clone)]
pub struct WinScript {
    /// A short name for error messages.
    pub name: &'static str,
    /// The script body, a `const`. It reads its parameters from `$p` and ends
    /// by calling `E` with the result.
    pub body: &'static str,
    /// The parameters: a JSON object (or `Null` for none), decoded in-script.
    pub params: Value,
}

impl WinScript {
    /// The script that replaces `dst` with `src` ([`MOVE_REPLACE`]).
    pub fn move_replace(src: &str, dst: &str) -> WinScript {
        WinScript {
            name: "move_replace",
            body: MOVE_REPLACE,
            params: serde_json::json!({ "src": src, "dst": dst }),
        }
    }

    /// The script that atomically replaces the release record `dst` with
    /// `src` ([`RECORD_REPLACE`]). `retries` is how often a sharing violation
    /// is retried; like both paths it travels only in the JSON parameters.
    pub fn record_replace(src: &str, dst: &str, retries: u32) -> WinScript {
        WinScript {
            name: "record_replace",
            body: RECORD_REPLACE,
            params: serde_json::json!({ "src": src, "dst": dst, "retries": retries }),
        }
    }

    /// The script that probes the `.git` entry of one project ([`GIT_ENTRY`]).
    /// The three native paths are `<data>\projects`, the project directory and
    /// its `.git`; they travel only as parameters.
    pub fn git_entry(projects_dir: &str, project_dir: &str, git: &str) -> WinScript {
        WinScript {
            name: "git_entry",
            body: GIT_ENTRY,
            params: serde_json::json!({ "a": projects_dir, "b": project_dir, "c": git }),
        }
    }

    /// The script that walks `roots` with `prefix` in front of each
    /// ([`ENUMERATE`]). `prefix` is [`LONG_PATH_PREFIX`] on a Windows host.
    pub fn enumerate(prefix: &str, roots: &[String]) -> WinScript {
        WinScript {
            name: "enumerate",
            body: ENUMERATE,
            params: serde_json::json!({ "x": prefix, "r": roots }),
        }
    }

    /// The script that lists the direct children of `roots` with `prefix` in
    /// front of each ([`ENUMERATE_CHILDREN`]); its result parses with
    /// [`parse_enumeration`].
    pub fn enumerate_children(prefix: &str, roots: &[String]) -> WinScript {
        WinScript {
            name: "enumerate_children",
            body: ENUMERATE_CHILDREN,
            params: serde_json::json!({ "x": prefix, "r": roots }),
        }
    }

    /// The script that zips exactly the files of the uploaded list `list`
    /// (native paths below `resources`) into `archive` ([`BUNDLE`]).
    /// `list_sha256` is the lowercase hex SHA-256 of the list bytes, `need`
    /// the free bytes the archive's drive must have. Every value travels only
    /// in the JSON parameters.
    pub fn bundle(
        prefix: &str,
        resources: &str,
        list: &str,
        archive: &str,
        list_sha256: &str,
        need: u64,
    ) -> WinScript {
        Self::bundle_level(prefix, resources, list, archive, list_sha256, need, None)
    }

    /// [`bundle`](Self::bundle) with the compression level of the entries:
    /// `Some("NoCompression")` stores them, `None` sends no `c` and keeps the
    /// `Optimal` of every existing read. `root` is the native directory the
    /// listed names are below.
    pub fn bundle_level(
        prefix: &str,
        root: &str,
        list: &str,
        archive: &str,
        list_sha256: &str,
        need: u64,
        level: Option<&str>,
    ) -> WinScript {
        let mut params = serde_json::json!({
            "x": prefix,
            "b": root,
            "l": list,
            "z": archive,
            "h": list_sha256,
            "f": need,
        });
        if let (Some(level), Some(map)) = (level, params.as_object_mut()) {
            map.insert("c".to_string(), Value::String(level.to_string()));
        }
        WinScript {
            name: "bundle",
            body: BUNDLE,
            params,
        }
    }

    /// The script that removes the files `names` and then the staging dir
    /// `dir` (native) ([`BUNDLE_CLEAN`]).
    pub fn bundle_clean(prefix: &str, dir: &str, names: &[&str]) -> WinScript {
        WinScript {
            name: "bundle_clean",
            body: BUNDLE_CLEAN,
            params: serde_json::json!({ "x": prefix, "d": dir, "f": names }),
        }
    }

    /// The script that reads the fixed files of the `.git` at `git` (native),
    /// asking for the presence of `markers` ([`GIT_STATE`]).
    pub fn git_state(git: &str, markers: &[&str]) -> WinScript {
        WinScript {
            name: "git_state",
            body: GIT_STATE,
            params: serde_json::json!({ "a": git, "m": markers }),
        }
    }

    /// The script that saves a project's `.git` ([`GIT_SAVE`]): `prefix` is
    /// [`LONG_PATH_PREFIX`], `data` the native data directory, `home` the
    /// native account home.
    pub fn git_save(
        prefix: &str,
        data: &str,
        project: &str,
        home: &str,
        gateway: &str,
        run_id: &str,
    ) -> WinScript {
        WinScript {
            name: "git_save",
            body: GIT_SAVE,
            params: git_run_params(prefix, data, project, home, gateway, run_id),
        }
    }

    /// The script that restores the save of a run ([`GIT_RESTORE`]); the
    /// parameters are [`git_save`](Self::git_save)'s.
    pub fn git_restore(
        prefix: &str,
        data: &str,
        project: &str,
        home: &str,
        gateway: &str,
        run_id: &str,
    ) -> WinScript {
        WinScript {
            name: "git_restore",
            body: GIT_RESTORE,
            params: git_run_params(prefix, data, project, home, gateway, run_id),
        }
    }

    /// The script that takes the manifest of the native directory `dir`
    /// ([`GIT_MANIFEST`]).
    pub fn git_manifest(prefix: &str, dir: &str) -> WinScript {
        WinScript {
            name: "git_manifest",
            body: GIT_MANIFEST,
            params: serde_json::json!({ "x": prefix, "a": dir }),
        }
    }

    /// The script that sizes the native directory `dir` from one enumeration
    /// ([`GIT_SIZE`]).
    pub fn git_size(prefix: &str, dir: &str) -> WinScript {
        WinScript {
            name: "git_size",
            body: GIT_SIZE,
            params: serde_json::json!({ "x": prefix, "a": dir }),
        }
    }

    /// The script that lists the object files of the native `.git` at `git`
    /// ([`GIT_OBJECTS`]): `walk_max` is the most entries it may walk and
    /// `report_max` the most it reports before it says `more`.
    pub fn git_objects(prefix: &str, git: &str, walk_max: usize, report_max: usize) -> WinScript {
        WinScript {
            name: "git_objects",
            body: GIT_OBJECTS,
            params: serde_json::json!({
                "x": prefix, "a": git, "l": walk_max, "m": report_max,
            }),
        }
    }

    /// The script that hashes `file` (a `/`-separated path) below the native
    /// `.git` ([`GIT_DIGEST`]).
    pub fn git_digest(git: &str, file: &str) -> WinScript {
        WinScript {
            name: "git_digest",
            body: GIT_DIGEST,
            params: serde_json::json!({ "a": git, "f": file }),
        }
    }

    /// The script that makes the private stage `stage` below `ign-deploy` of the
    /// native account `home` ([`GIT_STAGE`]).
    pub fn git_stage(home: &str, stage: &str) -> WinScript {
        WinScript {
            name: "git_stage",
            body: GIT_STAGE,
            params: serde_json::json!({ "h": home, "s": stage }),
        }
    }

    /// The script that creates and fills the locks of `writes` (`(path below
    /// .git, size)`), the first of which is plan index `offset` ([`GIT_LOCK`]).
    pub fn git_lock(
        data: &str,
        project: &str,
        home: &str,
        stage: &str,
        offset: usize,
        writes: &[(String, u64)],
    ) -> WinScript {
        WinScript {
            name: "git_lock",
            body: GIT_LOCK,
            params: serde_json::json!({
                "d": data, "j": project, "h": home, "s": stage, "o": offset,
                "w": Self::git_entries(writes),
            }),
        }
    }

    /// The script that removes the locks of `writes` that hold ign's own bytes,
    /// and with `remove_stage` the stage ([`GIT_UNLOCK`]).
    pub fn git_unlock(
        data: &str,
        project: &str,
        home: &str,
        stage: &str,
        offset: usize,
        writes: &[(String, u64)],
        remove_stage: bool,
    ) -> WinScript {
        WinScript {
            name: "git_unlock",
            body: GIT_UNLOCK,
            params: serde_json::json!({
                "d": data, "j": project, "h": home, "s": stage, "o": offset,
                "w": Self::git_entries(writes), "r": remove_stage,
            }),
        }
    }

    /// The script that puts each lock over its target ([`GIT_REPLACE`]):
    /// `writes` are `(path below .git, size sent)`.
    pub fn git_replace(
        data: &str,
        project: &str,
        home: &str,
        stage: &str,
        writes: &[(String, u64)],
        retries: u32,
        remove_stage: bool,
    ) -> WinScript {
        WinScript {
            name: "git_replace",
            body: GIT_REPLACE,
            params: serde_json::json!({
                "d": data, "j": project, "h": home, "s": stage,
                "w": Self::git_entries(writes), "q": retries, "r": remove_stage,
            }),
        }
    }

    fn git_entries(writes: &[(String, u64)]) -> Vec<String> {
        writes
            .iter()
            .map(|(path, size)| format!("{path}:{size}"))
            .collect()
    }

    /// The script that probes write access in a project directory (below the
    /// native `data` directory) and its `.git` with the file `name`
    /// ([`GIT_PROBE`]).
    pub fn git_probe(data: &str, project: &str, name: &str) -> WinScript {
        WinScript {
            name: "git_probe",
            body: GIT_PROBE,
            params: serde_json::json!({ "d": data, "j": project, "n": name }),
        }
    }

    /// The script that removes the save at `rel` (`/`-separated, below the
    /// native account `home`) of run `run_id` ([`GIT_SAVE_RM`]).
    pub fn git_save_rm(prefix: &str, home: &str, rel: &str, run_id: &str) -> WinScript {
        WinScript {
            name: "git_save_rm",
            body: GIT_SAVE_RM,
            params: serde_json::json!({ "x": prefix, "h": home, "r": rel, "i": run_id }),
        }
    }

    /// The script that reports the free bytes on the volume of the save
    /// directory `rel` below the native account `home` ([`GIT_FREE`]).
    pub fn git_free(home: &str, rel: &str) -> WinScript {
        WinScript {
            name: "git_free",
            body: GIT_FREE,
            params: serde_json::json!({ "h": home, "s": rel }),
        }
    }
}

/// The parameters of [`GIT_SAVE`] and [`GIT_RESTORE`].
fn git_run_params(
    prefix: &str,
    data: &str,
    project: &str,
    home: &str,
    gateway: &str,
    run_id: &str,
) -> Value {
    serde_json::json!({
        "x": prefix, "d": data, "j": project, "h": home, "g": gateway, "i": run_id,
    })
}

/// The remote command for `script`, or why it cannot be sent: the parameters
/// must be a JSON object (or `Null`), and the whole command must fit `budget`
/// characters ([`COMMAND_BUDGET`] in production). Built by
/// [`RemoteCommand::powershell_encoded`]; this function only composes the
/// script text.
pub fn encoded_command(script: &WinScript, budget: usize) -> Result<RemoteCommand, String> {
    use base64::Engine as _;
    let mut text = String::from(PRELUDE);
    match &script.params {
        Value::Null => {}
        Value::Object(map) if map.is_empty() => {}
        Value::Object(_) => {
            let json = serde_json::to_vec(&script.params).map_err(|e| {
                format!("the parameters of {} cannot be encoded ({e})", script.name)
            })?;
            let blob = base64::engine::general_purpose::STANDARD.encode(json);
            text.push_str("$p=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('");
            text.push_str(&blob);
            text.push_str("'))|ConvertFrom-Json;");
        }
        _ => {
            return Err(format!(
                "the parameters of {} must be a JSON object",
                script.name
            ));
        }
    }
    text.push_str(script.body);
    let command = RemoteCommand::powershell_encoded(&text);
    let length = command.as_str().chars().count();
    if length > budget {
        return Err(format!(
            "the remote command for {} is {length} characters, over the {budget} character budget",
            script.name
        ));
    }
    Ok(command)
}

/// The JSON of the single [`IGN_RESULT_SENTINEL`] line in `stdout`. Lines
/// without the sentinel are ignored; no such line, two or more, or text after
/// the sentinel that is not JSON is an error.
pub fn parse_ign_result(stdout: &[u8]) -> Result<Value, String> {
    let mut found: Option<&[u8]> = None;
    for line in stdout.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(rest) = line.strip_prefix(IGN_RESULT_SENTINEL.as_bytes()) {
            if found.is_some() {
                return Err(format!(
                    "the output holds more than one {IGN_RESULT_SENTINEL} line"
                ));
            }
            found = Some(rest);
        }
    }
    let Some(json) = found else {
        return Err(format!("the output holds no {IGN_RESULT_SENTINEL} line"));
    };
    serde_json::from_slice(json)
        .map_err(|_| format!("the {IGN_RESULT_SENTINEL} line is not valid JSON"))
}

/// The messages of the `<S S="Error">` strings in a CLIXML stderr, decoded
/// for display only. Empty when `stderr` is not CLIXML.
pub fn clixml_errors(stderr: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(stderr);
    if !text.contains("CLIXML") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut rest: &str = &text;
    while let Some(at) = rest.find("<S S=\"Error\">") {
        rest = &rest[at + "<S S=\"Error\">".len()..];
        let Some(end) = rest.find("</S>") else { break };
        let raw = &rest[..end];
        rest = &rest[end..];
        let decoded = raw
            .replace("_x000D__x000A_", "\n")
            .replace("_x000A_", "\n")
            .replace("_x000D_", "")
            .replace("_x0009_", "\t")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        let decoded = decoded.trim();
        if !decoded.is_empty() {
            out.push(decoded.to_string());
        }
    }
    out
}

/// The result of one [`WinScript`] run, or the detail of the
/// `transport_failed` it is. Success needs exit 0 within the timeout, exactly
/// one sentinel line, and no error record on stderr; stderr is otherwise
/// opaque (see the module doc).
pub fn interpret_output(
    out: &ProcessOutput,
    timeout_s: u64,
    temp_dir: Option<&std::path::Path>,
) -> Result<Value, String> {
    if out.timed_out {
        return Err(format!("timed out after {timeout_s}s and was killed"));
    }
    let explain = || -> String {
        let decoded = clixml_errors(&out.stderr);
        if decoded.is_empty() {
            scrub_stderr(&out.stderr, temp_dir)
        } else {
            scrub_stderr(decoded.join("\n").as_bytes(), temp_dir)
        }
    };
    if out.code != 0 {
        let stderr_text = String::from_utf8_lossy(&out.stderr);
        if stderr_text.contains("The command line is too long") {
            return Err(format!(
                "exit code {}: the host refused the remote command as too long (budget {COMMAND_BUDGET} characters)",
                out.code
            ));
        }
        let tail = explain();
        return Err(if tail.is_empty() {
            format!("exit code {}", out.code)
        } else {
            format!("exit code {}: {tail}", out.code)
        });
    }
    let errors = clixml_errors(&out.stderr);
    if !errors.is_empty() {
        return Err(format!(
            "the script exited 0 but wrote an error record: {}",
            scrub_stderr(errors.join("\n").as_bytes(), temp_dir)
        ));
    }
    parse_ign_result(&out.stdout)
}

/// One entry of an [`ENUMERATE`] result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumEntry {
    /// Path below the walked root, `/`-separated.
    pub rel: String,
    /// A directory.
    pub dir: bool,
    /// File size (0 for a directory).
    pub size: u64,
    /// A reparse point (junction or symlink).
    pub reparse: bool,
    /// Length of the entry's absolute path, prefix included.
    pub full_len: usize,
}

/// The entries per root of an [`ENUMERATE`] result: `None` for a root that
/// does not exist. `expected_roots` roots must be reported, in order.
pub fn parse_enumeration(
    value: &Value,
    expected_roots: usize,
) -> Result<Vec<Option<Vec<EnumEntry>>>, String> {
    let bad = |why: &str| format!("the enumeration result is malformed ({why})");
    if value.get("ok").and_then(Value::as_i64) != Some(1) {
        return Err(bad("ok is not 1"));
    }
    let res = value
        .get("res")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("res is not an array"))?;
    if res.len() != expected_roots {
        return Err(bad("the number of roots differs"));
    }
    let mut out = Vec::with_capacity(res.len());
    for (index, root) in res.iter().enumerate() {
        if root.get("i").and_then(Value::as_u64) != Some(index as u64) {
            return Err(bad("roots are out of order"));
        }
        if root.get("x").and_then(Value::as_i64) == Some(0) {
            out.push(None);
            continue;
        }
        let entries = root
            .get("e")
            .and_then(Value::as_array)
            .ok_or_else(|| bad("e is not an array"))?;
        let mut list = Vec::with_capacity(entries.len());
        for entry in entries {
            let cols = entry
                .as_array()
                .ok_or_else(|| bad("an entry is not an array"))?;
            let (Some(rel), Some(dir), Some(size), Some(reparse), Some(full_len)) = (
                cols.first().and_then(Value::as_str),
                cols.get(1).and_then(Value::as_i64),
                cols.get(2).and_then(Value::as_u64),
                cols.get(3).and_then(Value::as_i64),
                cols.get(4).and_then(Value::as_u64),
            ) else {
                return Err(bad("an entry has the wrong columns"));
            };
            // every path the host reports passes the same gate as any other
            // path that can reach a batch line
            let rel = rel.replace('\\', "/");
            for component in rel.split('/') {
                if let Some(why) = windows_component_problem(component) {
                    return Err(bad(&format!("an entry path is refused: {why}")));
                }
            }
            list.push(EnumEntry {
                rel,
                dir: dir != 0,
                size,
                reparse: reparse != 0,
                full_len: full_len as usize,
            });
        }
        out.push(Some(list));
    }
    Ok(out)
}

/// Split `roots` into the fewest [`ENUMERATE`] commands that each fit
/// [`COMMAND_BUDGET`]: the index ranges of `roots` and the script for each. A
/// single root that does not fit alone is an error.
pub fn enumerate_chunks(
    prefix: &str,
    roots: &[String],
) -> Result<Vec<(std::ops::Range<usize>, WinScript)>, String> {
    chunks_by_budget(prefix, roots, WinScript::enumerate)
}

/// [`enumerate_chunks`] for [`ENUMERATE_CHILDREN`] (quick 261104-s3n): the
/// fewest direct-children listings of `roots` that each fit
/// [`COMMAND_BUDGET`].
pub fn enumerate_children_chunks(
    prefix: &str,
    roots: &[String],
) -> Result<Vec<(std::ops::Range<usize>, WinScript)>, String> {
    chunks_by_budget(prefix, roots, WinScript::enumerate_children)
}

/// The budget split behind [`enumerate_chunks`] and
/// [`enumerate_children_chunks`], over the script `build` makes.
fn chunks_by_budget(
    prefix: &str,
    roots: &[String],
    build: fn(&str, &[String]) -> WinScript,
) -> Result<Vec<(std::ops::Range<usize>, WinScript)>, String> {
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < roots.len() {
        let mut end = start + 1;
        if encoded_command(&build(prefix, &roots[start..end]), COMMAND_BUDGET).is_err() {
            return Err(format!(
                "the path {:?} is too long to enumerate within the remote command budget",
                roots[start]
            ));
        }
        while end < roots.len()
            && encoded_command(&build(prefix, &roots[start..=end]), COMMAND_BUDGET).is_ok()
        {
            end += 1;
        }
        chunks.push((start..end, build(prefix, &roots[start..end])));
        start = end;
    }
    Ok(chunks)
}

/// The three native absolute roots the uploaded script works in (Phase 23,
/// 23-08): the run's staging directory (it holds `commit.ps1` and
/// `work.tsv`), the mode collection and the `ign-deploy/<gateway>/<mode>`
/// directory. Native form (`C:\Users\...`), not the sftp spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StubRoots {
    /// The directory that holds the script, the work list and the staged files.
    pub staging: String,
    /// `<data dir>\config\resources\<mode>`.
    pub collection: String,
    /// `<home>\ign-deploy\<gateway>\<mode>`.
    pub deploy: String,
}

/// The fixed body of the stub that runs the uploaded script (D-01, C-script-run).
///
/// It reads `commit.ps1` and `work.tsv` from the staging directory ONCE each,
/// hashes the bytes it read with SHA-256 and compares them with the values the
/// runner computed (`$p.ss`, `$p.ws`). On a mismatch it writes one fixed
/// sentence to stderr and exits 97
/// ([`STUB_MISMATCH_EXIT`](super::remote::STUB_MISMATCH_EXIT)) before it runs
/// anything. Otherwise it runs
/// the script text it hashed through `[ScriptBlock]::Create` (the execution
/// policy does not govern a script block, so the same stub works under
/// `Restricted` and `AllSigned`), passing the operation, the three roots and
/// the text of the work list it hashed, so that the list the script acts on is
/// the list that was checked. No path, hash or reason is part of the body:
/// they travel in the base64 JSON parameters.
pub const STUB: &str = concat!(
    "$q='\\\\?\\';",
    "$b=[IO.File]::ReadAllBytes($q+$p.t+'\\commit.ps1');",
    "$w=[IO.File]::ReadAllBytes($q+$p.t+'\\work.tsv');",
    "$a=[Security.Cryptography.SHA256]::Create();",
    "$x=[BitConverter]::ToString($a.ComputeHash($b)).Replace('-','').ToLower();",
    "$y=[BitConverter]::ToString($a.ComputeHash($w)).Replace('-','').ToLower();",
    "if($x -cne $p.ss -or $y -cne $p.ws){",
    "[Console]::Error.WriteLine('ign: the staged script or work list does not match its hash');exit 97};",
    "& ([ScriptBlock]::Create([Text.Encoding]::UTF8.GetString($b))) ",
    "$p.o $p.t $p.c $p.d ([Text.Encoding]::UTF8.GetString($w))"
);

/// The [`WinScript`] that verifies the uploaded `commit.ps1` and `work.tsv`
/// against `script_sha256` and `work_sha256` and then runs the script's `op`
/// over `roots` ([`STUB`]). The staging directory of `roots` must hold both
/// files. Paths travel only in the parameters; the command is held to
/// [`COMMAND_BUDGET`] by `exec_powershell`.
pub fn script_stub(
    script_sha256: &str,
    work_sha256: &str,
    op: &str,
    roots: &StubRoots,
) -> WinScript {
    WinScript {
        name: "script_stub",
        body: STUB,
        params: serde_json::json!({
            "t": roots.staging,
            "c": roots.collection,
            "d": roots.deploy,
            "o": op,
            "ss": script_sha256,
            "ws": work_sha256,
        }),
    }
}

/// Cuts `line` every 119 characters with no indent: how the capture host's
/// PowerShell 5.1 wraps an error line (22-LIVE-CAPTURES.md line 7223, chunks
/// of 119, 119, 119 and 1 character).
#[cfg(test)]
fn ps51_wrap(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    chars
        .chunks(119)
        .map(|piece| piece.iter().collect())
        .collect()
}

/// A `+ ...` line of an error record: the text is split into 114-character
/// pieces, each piece gets 4 spaces in front and the pieces are joined with
/// nothing between. Cut by [`ps51_wrap`], a 123-character CategoryInfo text
/// gives a 119-character chunk ending in a space and then a chunk of 3 spaces
/// and the rest (capture line 7223).
#[cfg(test)]
fn ps51_info_line(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(114)
        .map(|piece| format!("    {}", piece.iter().collect::<String>()))
        .collect()
}

/// The CategoryInfo target: the whole text when it has 40 characters or
/// fewer, otherwise its first 15 characters, `...` and its last 15 (the 15/15
/// cut is on capture line 7223; the 40-character threshold is in no capture).
#[cfg(test)]
fn ps51_ellipsize(target: &str) -> String {
    let chars: Vec<char> = target.chars().collect();
    if chars.len() <= 40 {
        return target.to_string();
    }
    let head: String = chars[..15].iter().collect();
    let tail: String = chars[chars.len() - 15..].iter().collect();
    format!("{head}...{tail}")
}

/// The CLIXML stderr of Windows PowerShell 5.1 for `throw '<thrown>'`, in the
/// shape captured on the capture host: `#< CLIXML`, CRLF, the `<Objs>` frame
/// and one `<S S="Error">` chunk per output line, each ending
/// `_x000D__x000A_`. Checked byte for byte against the records of
/// 22-LIVE-CAPTURES.md lines 4751-4752 and 4767-4768 (a thrown string) and
/// line 7223 (the 119-column wrap, the wrapped `+` line and the target
/// ellipsis) by `ps51_error_clixml_matches_the_captured_records`.
///
/// One part stays a model: a FullyQualifiedErrorId line longer than one
/// 114-character piece has no capture. The live Add-Type cause test proves it,
/// because its thrown text spans three pieces on a real host.
#[cfg(test)]
pub(crate) fn ps51_error_clixml(thrown: &str) -> Vec<u8> {
    let mut lines = ps51_wrap(thrown);
    lines.push("At line:1 char:1200".to_string());
    lines.push("+ ... $n -ge [int]$p.retries){throw ('the record replace ...".to_string());
    lines.push(format!("+ {}{}", " ".repeat(30), "~".repeat(12)));
    lines.extend(ps51_wrap(&ps51_info_line(&format!(
        "+ CategoryInfo          : OperationStopped: ({}:String) [], RuntimeException",
        ps51_ellipsize(thrown)
    ))));
    lines.extend(ps51_wrap(&ps51_info_line(&format!(
        "+ FullyQualifiedErrorId : {thrown}"
    ))));
    lines.push(" ".to_string());
    let mut out = String::from(
        "#< CLIXML\r\n<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\">",
    );
    for line in &lines {
        let escaped = line
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out.push_str(&format!("<S S=\"Error\">{escaped}_x000D__x000A_</S>"));
    }
    out.push_str("</Objs>");
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn decode_command(command: &RemoteCommand) -> String {
        let text = command.as_str();
        let b64 = text
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .expect("the fixed powershell prefix");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64");
        assert_eq!(bytes.len() % 2, 0, "UTF-16LE has an even length");
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&units).expect("utf-16")
    }

    fn script(params: Value) -> WinScript {
        WinScript {
            name: "t",
            body: "E @{ok=1}",
            params,
        }
    }

    #[test]
    fn encoded_command_is_utf16le_base64() {
        let command = encoded_command(&script(Value::Null), COMMAND_BUDGET).expect("command");
        let text = decode_command(&command);
        assert_eq!(text, format!("{PRELUDE}E @{{ok=1}}"));
        assert!(
            text.starts_with(
                "$ProgressPreference='SilentlyContinue';$ErrorActionPreference='Stop';"
            )
        );
        // only base64 characters reach the shell after the fixed prefix
        let b64 = command
            .as_str()
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .expect("prefix");
        assert!(
            b64.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        );
    }

    #[test]
    fn parameters_travel_as_a_base64_json_blob_never_interpolated() {
        let hostile = "C:\\a b\\'; Remove-Item -Recurse C:\\ ; '$(whoami)`\"";
        let command = encoded_command(
            &script(serde_json::json!({ "path": hostile })),
            COMMAND_BUDGET,
        )
        .expect("command");
        let text = decode_command(&command);
        assert!(
            !text.contains("Remove-Item"),
            "the hostile text must not appear in the script: {text}"
        );
        assert!(!text.contains("whoami"));
        let blob = text
            .split("FromBase64String('")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .expect("blob");
        let json = base64::engine::general_purpose::STANDARD
            .decode(blob)
            .expect("blob base64");
        let value: Value = serde_json::from_slice(&json).expect("json");
        assert_eq!(value["path"], hostile);
        // parameters must be an object
        for bad in [
            serde_json::json!([1]),
            serde_json::json!("x"),
            serde_json::json!(3),
        ] {
            assert!(encoded_command(&script(bad), COMMAND_BUDGET).is_err());
        }
    }

    #[test]
    fn encoded_command_budget() {
        let exact = encoded_command(&script(Value::Null), COMMAND_BUDGET).expect("fits");
        let length = exact.as_str().chars().count();
        assert!(encoded_command(&script(Value::Null), length).is_ok());
        let err = encoded_command(&script(Value::Null), length - 1).expect_err("one over");
        assert!(err.contains("over the") && err.contains("budget"), "{err}");

        // a parameter blob too big for the budget is refused before any spawn
        let big = "x".repeat(COMMAND_BUDGET);
        assert!(encoded_command(&script(serde_json::json!({ "p": big })), COMMAND_BUDGET).is_err());
        // the shipped scripts leave room for their parameters
        let enumerate = encoded_command(
            &WinScript::enumerate(
                LONG_PATH_PREFIX,
                &[
                    "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\production\\ignition\\tag-definition\\provider".to_string(),
                ],
            ),
            COMMAND_BUDGET,
        )
        .expect("one realistic root fits");
        assert!(enumerate.as_str().len() < COMMAND_BUDGET);
    }

    #[test]
    fn enumerate_chunks_split_by_budget_and_cover_every_root() {
        let root = |i: usize| {
            format!(
                "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\production\\ignition\\tag-definition\\provider-{i:04}"
            )
        };
        let roots: Vec<String> = (0..60).map(root).collect();
        let chunks = enumerate_chunks(LONG_PATH_PREFIX, &roots).expect("chunks");
        assert!(chunks.len() >= 2, "60 long roots cannot share one command");
        let mut next = 0;
        for (range, script) in &chunks {
            assert_eq!(range.start, next, "contiguous");
            next = range.end;
            assert!(
                encoded_command(script, COMMAND_BUDGET).is_ok(),
                "each chunk fits the budget"
            );
        }
        assert_eq!(next, roots.len(), "every root is covered");
        let huge = vec!["C:\\".to_string() + &"d".repeat(COMMAND_BUDGET)];
        assert!(enumerate_chunks(LONG_PATH_PREFIX, &huge).is_err());
        assert!(
            enumerate_chunks(LONG_PATH_PREFIX, &[])
                .expect("none")
                .is_empty()
        );
    }

    /// quick 261104-s3n: the direct-children listing splits by the same
    /// budget, every chunk is the non-recursive script, and every root is
    /// covered once, in order.
    #[test]
    fn enumerate_children_chunks_split_by_budget_and_cover_every_root() {
        let root = |i: usize| {
            format!(
                "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\production\\ignition\\tag-definition\\provider-{i:04}"
            )
        };
        let roots: Vec<String> = (0..60).map(root).collect();
        let chunks = enumerate_children_chunks(LONG_PATH_PREFIX, &roots).expect("chunks");
        assert!(chunks.len() >= 2, "60 long roots cannot share one command");
        let mut next = 0;
        for (range, script) in &chunks {
            assert_eq!(range.start, next, "contiguous");
            next = range.end;
            assert_eq!(script.body, ENUMERATE_CHILDREN);
            assert_eq!(
                script.params["r"].as_array().map(Vec::len),
                Some(range.len())
            );
            assert!(encoded_command(script, COMMAND_BUDGET).is_ok());
        }
        assert_eq!(next, roots.len(), "every root is covered");
        // the ENUMERATE split is unchanged and uses the recursive script
        for (_, script) in enumerate_chunks(LONG_PATH_PREFIX, &roots).expect("chunks") {
            assert_eq!(script.body, ENUMERATE);
        }
        let huge = vec!["C:\\".to_string() + &"d".repeat(COMMAND_BUDGET)];
        assert!(enumerate_children_chunks(LONG_PATH_PREFIX, &huge).is_err());
        assert!(
            enumerate_children_chunks(LONG_PATH_PREFIX, &[])
                .expect("none")
                .is_empty()
        );
    }

    /// T-25-40, T-25-46, T-25-42: the record replace is fixed one-line ASCII
    /// text that asserts FullLanguage, declares the `MoveFileEx` P/Invoke once
    /// inside a try whose catch throws, never names `Move-Item` or a path, and
    /// takes its paths and retry count only from the JSON blob.
    #[test]
    fn record_replace_is_fixed_fail_closed_text() {
        let body = RECORD_REPLACE;
        assert!(!body.is_empty());
        assert!(
            body.bytes().all(|b| (0x20..=0x7e).contains(&b)),
            "one line of printable ASCII"
        );
        assert!(!body.contains("ign-deploy") && !body.contains("C:\\"));
        assert!(
            !body.contains("Move-Item"),
            "not the delete-then-rename cmdlet"
        );
        assert!(body.contains("$ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage'"));
        assert!(body.contains("MoveFileEx(string a,string b,uint c)"));
        assert!(body.contains("kernel32.dll") && body.contains("SetLastError=true"));
        assert!(
            body.contains("0x1 -bor 0x8"),
            "REPLACE_EXISTING | WRITE_THROUGH"
        );
        assert!(body.contains("GetLastWin32Error"));
        assert!(body.contains("$p.src") && body.contains("$p.dst") && body.contains("$p.retries"));
        // the type-exists guard comes first and there is a single Add-Type
        let guard = body.find("'IgnRecord.Native' -as [type]").expect("guard");
        assert_eq!(body.matches("Add-Type").count(), 1);
        let add = body.find("Add-Type").expect("Add-Type");
        assert!(guard < add, "the guard precedes the declaration");
        let try_at = body[..add].rfind("try{").expect("try before Add-Type");
        let catch_at = body[add..].find("catch{throw ").expect("catch rethrows") + add;
        assert!(guard < try_at && try_at < add && add < catch_at);

        // sized for realistic account-home paths, plain and accented
        let long = format!("C:\\Users\\svc\\{}", "d".repeat(227));
        assert_eq!(long.len(), 240);
        let accented = format!("C:\\Users\\sv\u{e7}\\{}", "\u{e9}".repeat(110));
        for path in [&long, &accented] {
            assert!(
                encoded_command(
                    &WinScript::record_replace(
                        path,
                        path,
                        crate::deploy::transport::batch::RENAME_RETRIES
                    ),
                    COMMAND_BUDGET
                )
                .is_ok(),
                "fits the budget"
            );
        }

        // the paths and the count reach the script only through the blob
        let (src, dst) = ("C:\\h\\a.tmp-r1", "C:\\h\\a.json");
        let split = |retries: u32| {
            let text = decode_command(
                &encoded_command(
                    &WinScript::record_replace(src, dst, retries),
                    COMMAND_BUDGET,
                )
                .expect("command"),
            );
            let blob = text
                .split("FromBase64String('")
                .nth(1)
                .and_then(|rest| rest.split('\'').next())
                .expect("blob")
                .to_string();
            let json = base64::engine::general_purpose::STANDARD
                .decode(&blob)
                .expect("blob base64");
            let value: Value = serde_json::from_slice(&json).expect("json");
            (text.replace(&blob, "<blob>"), value)
        };
        let (text3, value3) = split(3);
        let (text7, value7) = split(7);
        assert_eq!(
            value3,
            serde_json::json!({ "src": src, "dst": dst, "retries": 3 })
        );
        assert_eq!(
            value7,
            serde_json::json!({ "src": src, "dst": dst, "retries": 7 })
        );
        assert!(value3["retries"].is_number());
        assert_eq!(
            text3, text7,
            "the text outside the blob does not depend on the count"
        );
        assert!(text3.contains("$p.retries"));
        assert!(!text3.contains(src) && !text3.contains(dst));
        assert!(text3.ends_with(RECORD_REPLACE));
    }

    /// Whitespace-free form: PowerShell's 119-column wrap can fall anywhere
    /// and `clixml_errors` trims each chunk, so texts are compared squashed.
    fn squash(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// T-25-46, T-25-67, T-25-68: every thrown text is a bounded, single-line,
    /// printable-ASCII sentence built from the three fixed prefixes, and the
    /// 25-20 mechanism is still there byte for byte.
    #[test]
    fn record_replace_cause_is_bounded_single_line_ascii() {
        let body = RECORD_REPLACE;
        assert!(
            body.bytes().all(|b| (0x20..=0x7e).contains(&b)),
            "one line of printable ASCII"
        );
        let prefixes = [
            "the record replace needs a FullLanguage PowerShell session, not ",
            "the record replace cannot declare MoveFileEx: ",
            "the record replace failed with Win32 error ",
        ];
        for prefix in prefixes {
            assert!(body.contains(prefix), "{prefix}");
            // the cause is at most 200 characters + `...`, the Win32 number
            // at most 11 characters + `: ` (the language mode is at most 19)
            assert!(prefix.len() + 203 + 13 <= 300, "{prefix}");
        }
        let language = body
            .find("$ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage'")
            .expect("language check");
        let line = body.find("function IgnLine").expect("IgnLine");
        let cause = body.find("function IgnCause").expect("IgnCause");
        let guard = body.find("'IgnRecord.Native' -as [type]").expect("guard");
        assert!(language < line && line < cause && cause < guard);
        let cause_body = &body[cause..guard];
        assert!(cause_body.contains("try{") && cause_body.contains("catch{"));
        assert!(
            cause_body.contains("catch{'the cause could not be read'}"),
            "a failing helper returns a fixed sentence"
        );
        assert!(!body[line..guard].contains("Add-Type"));
        // the cause is concatenated into a string and never evaluated
        let lower = body.to_ascii_lowercase();
        for banned in ["invoke-expression", "iex ", "scriptblock", "[scriptblock]"] {
            assert!(!lower.contains(banned), "{banned}");
        }
        // 25-20 mechanism segments, unchanged
        for segment in [
            "if(-not ('IgnRecord.Native' -as [type])){try{Add-Type -Namespace IgnRecord -Name Native -MemberDefinition '[DllImport(\"kernel32.dll\",SetLastError=true,CharSet=CharSet.Unicode)]public static extern bool MoveFileEx(string a,string b,uint c);'}catch{throw ",
            "$f=[uint32](0x1 -bor 0x8);$n=0;",
            "while(-not [IgnRecord.Native]::MoveFileEx($p.src,$p.dst,$f)){$c=[Runtime.InteropServices.Marshal]::GetLastWin32Error();",
            "if(($c -ne 32 -and $c -ne 5) -or $n -ge [int]$p.retries){throw ",
            "$n++;Start-Sleep -Seconds 1};E @{ok=1}",
        ] {
            assert!(body.contains(segment), "{segment}");
        }
        assert!(body.ends_with("$n++;Start-Sleep -Seconds 1};E @{ok=1}"));
        // no failure branch reaches the success line other than the loop exit
        assert_eq!(body.matches("E @{ok=1}").count(), 1);
    }

    /// A thrown text of exactly `len` characters, made of numbered words.
    fn thrown_text(len: usize, special: bool) -> String {
        let mut text = String::from("the record replace failed with Win32 error 32: ");
        if special {
            text.push_str("<path> it's a & b ");
        }
        let mut n = 0;
        while text.len() < len {
            text.push_str(&format!("w{n} "));
            n += 1;
        }
        text.truncate(len);
        text
    }

    /// WR-01: the cause survives the captured PowerShell 5.1 error record and
    /// the 5-line stderr tail, whole, up to the 300-character bound.
    #[test]
    fn record_replace_cause_survives_the_clixml_tail() {
        let failed = |thrown: &str| -> String {
            let stderr = ps51_error_clixml(thrown);
            let output = ProcessOutput {
                stdout: Vec::new(),
                stderr,
                code: 1,
                timed_out: false,
            };
            interpret_output(&output, 60, None).expect_err("a non-zero exit is an error")
        };
        for (len, special) in [(60, false), (150, false), (150, true), (299, true)] {
            let thrown = thrown_text(len, special);
            assert_eq!(thrown.len(), len);
            let detail = failed(&thrown);
            assert!(
                squash(&detail).contains(&squash(&thrown)),
                "{len}: {detail}"
            );
            assert!(
                !detail.contains("<S S=") && !detail.contains("&lt;"),
                "{detail}"
            );
            if special {
                assert!(
                    detail.contains("<path>") && detail.contains('&'),
                    "{detail}"
                );
            }
        }
        // the boundary: exactly 300 characters fills the 5-line tail and is whole
        let edge = thrown_text(300, false);
        assert_eq!(edge.chars().count(), 300);
        let detail = failed(&edge);
        assert!(squash(&detail).contains(&squash(&edge)), "{detail}");
        let tail = detail
            .strip_prefix("exit code 1: ")
            .expect("exit code prefix");
        assert_eq!(tail.lines().count(), 5, "{detail}");
        // control: past the bound the tail cuts, which is why the bound matters
        let long = thrown_text(700, false);
        let detail = failed(&long);
        assert!(!squash(&detail).contains(&squash(&long)), "{detail}");
    }

    // The records below are copied byte for byte from
    // .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md. The
    // capture tool removed one CR from each, so the raw bytes are the
    // `#< CLIXML` line, CRLF, then the `<Objs>` line with no trailing newline.
    /// Lines 4751-4752: `throw 'boom'`, bare.
    const CAPTURED_BOOM_BARE: &str = concat!(
        "#< CLIXML",
        "\r\n",
        r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">boom_x000D__x000A_</S><S S="Error">At line:1 char:1_x000D__x000A_</S><S S="Error">+ throw 'boom'_x000D__x000A_</S><S S="Error">+ ~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : boom_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>"#
    );
    /// Lines 4767-4768: `throw 'boom'` after the prelude.
    const CAPTURED_BOOM_PRELUDE: &str = concat!(
        "#< CLIXML",
        "\r\n",
        r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">boom_x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... rence='SilentlyContinue'; $ErrorActionPreference='Stop'; throw 'boom'_x000D__x000A_</S><S S="Error">+                                                              ~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : boom_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>"#
    );
    /// Line 7223 alone: a Get-ChildItem error on a long path.
    const CAPTURED_WRAPPED: &str = r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">Get-ChildItem : Could not find a part of the path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources_x000D__x000A_</S><S S="Error">\core\ignition\tag-definition\CAP22H\LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL_x000D__x000A_</S><S S="Error">LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL'_x000D__x000A_</S><S S="Error">._x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... nce='Stop'; Get-ChildItem -LiteralPath (Join-Path 'C:\Program Files\I ..._x000D__x000A_</S><S S="Error">+                 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : ReadError: (C:\Program File...LLLLLLLLLLLLLLL:String) [Get-ChildItem], DirectoryNotFound _x000D__x000A_</S><S S="Error">   Exception_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : DirIOError,Microsoft.PowerShell.Commands.GetChildItemCommand_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>"#;

    /// The raw `<S S="Error">` chunk bodies of a CLIXML record, with the
    /// `_x000D__x000A_` ends left on.
    fn raw_chunks(clixml: &str) -> Vec<String> {
        clixml
            .split("<S S=\"Error\">")
            .skip(1)
            .map(|piece| piece.split("</S>").next().expect("chunk").to_string())
            .collect()
    }

    const CRLF_ENTITY: &str = "_x000D__x000A_";

    /// The offline proof that `ps51_error_clixml` is the PowerShell 5.1 shape
    /// and not only a model of it: the cause-survival test and the writer
    /// mock test rest on it (D-03, D-06, D-12).
    #[test]
    fn ps51_error_clixml_matches_the_captured_records() {
        for captured in [CAPTURED_BOOM_BARE, CAPTURED_BOOM_PRELUDE] {
            let built = String::from_utf8(ps51_error_clixml("boom")).expect("utf-8");
            let head = |text: &str| text.split("<S S=").next().expect("head").to_string();
            assert_eq!(head(&built), head(captured), "framing through <Objs>");
            assert!(built.ends_with("</Objs>") && captured.ends_with("</Objs>"));
            assert!(!built.ends_with('\n') && !captured.ends_with('\n'));
            let (mine, theirs) = (raw_chunks(&built), raw_chunks(captured));
            assert_eq!((mine.len(), theirs.len()), (7, 7));
            for at in [0, 4, 5, 6] {
                assert_eq!(mine[at], theirs[at], "chunk {}", at + 1);
                assert!(mine[at].ends_with(CRLF_ENTITY));
            }
            // chunks 2 to 4 depend on the script position: the form only
            for chunk in [&mine, &theirs] {
                let digits = chunk[1]
                    .strip_prefix("At line:1 char:")
                    .and_then(|rest| rest.strip_suffix(CRLF_ENTITY))
                    .expect("At line form");
                assert!(!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));
                assert!(chunk[2].starts_with("+ ") && chunk[2].ends_with(CRLF_ENTITY));
                let marks = chunk[3]
                    .strip_prefix('+')
                    .and_then(|rest| rest.strip_suffix(CRLF_ENTITY))
                    .expect("underline form");
                assert!(marks.chars().all(|c| c == ' ' || c == '~'));
                assert!(marks.contains('~'));
            }
            let last_two = |text: &str| {
                let errors = clixml_errors(text.as_bytes());
                errors[errors.len() - 2..].to_vec()
            };
            assert_eq!(last_two(&built), last_two(captured));
            assert_eq!(
                last_two(captured),
                [
                    "+ CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException",
                    "+ FullyQualifiedErrorId : boom",
                ]
            );
        }

        // line 7223: the message wrap, the wrapped + line, the ellipsis
        let captured = raw_chunks(CAPTURED_WRAPPED);
        assert_eq!(captured.len(), 11);
        let bare: Vec<String> = captured
            .iter()
            .map(|chunk| chunk.strip_suffix(CRLF_ENTITY).expect("end").to_string())
            .collect();
        let message = bare[..4].concat();
        assert_eq!(
            bare[..4]
                .iter()
                .map(|c| c.chars().count())
                .collect::<Vec<_>>(),
            [119, 119, 119, 1]
        );
        assert_eq!(ps51_wrap(&message), bare[..4]);
        let quoted = message
            .split_once("the path '")
            .and_then(|(_, rest)| rest.strip_suffix("'."))
            .expect("quoted path");
        let ellipsis = ps51_ellipsize(quoted);
        assert_eq!(ellipsis, "C:\\Program File...LLLLLLLLLLLLLLL");
        let category = format!(
            "+ CategoryInfo          : ReadError: ({ellipsis}:String) [Get-ChildItem], DirectoryNotFoundException"
        );
        assert_eq!(ps51_wrap(&ps51_info_line(&category)), bare[7..9]);
        assert_eq!(bare[7].chars().count(), 119);
        assert!(bare[7].ends_with(' ') && bare[8].starts_with("   Exception"));
        let id = "+ FullyQualifiedErrorId : DirIOError,Microsoft.PowerShell.Commands.GetChildItemCommand";
        assert_eq!(ps51_wrap(&ps51_info_line(id)), bare[9..10]);
        assert_eq!(bare[10], " ");
        // the short-target rule: nothing is cut at 40 characters or fewer
        assert_eq!(ps51_ellipsize("boom"), "boom");
    }

    #[test]
    fn parse_ign_result_cases() {
        let ok = parse_ign_result(b"noise\r\nIGN-RESULT:{\"ok\":1}\r\n").expect("one line");
        assert_eq!(ok["ok"], 1);
        // a result with no trailing newline
        assert!(parse_ign_result(b"IGN-RESULT:{\"ok\":1}").is_ok());
        for (bad, why) in [
            (&b""[..], "no line"),
            (b"hello\n", "sentinel absent"),
            (b"IGN-RESULT:{\"a\":1}\nIGN-RESULT:{\"a\":2}\n", "two lines"),
            (b"IGN-RESULT:{nope\n", "invalid JSON"),
            (b"IGN-RESULT:\n", "empty payload"),
            (
                b" IGN-RESULT:{\"ok\":1}\n",
                "indented sentinel is not a sentinel line",
            ),
        ] {
            assert!(parse_ign_result(bad).is_err(), "{why} must be an error");
        }
    }

    fn out(code: i32, stdout: &str, stderr: &str, timed_out: bool) -> ProcessOutput {
        ProcessOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            code,
            timed_out,
        }
    }

    const CLIXML_ERROR: &str = "#< CLIXML\r\n<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\"><S S=\"Error\">Cannot find path 'C:\\x' because it does not exist._x000D__x000A_</S><S S=\"Error\">At line:1 char:1_x000D__x000A_</S></Objs>";
    const CLIXML_PROGRESS: &str = "#< CLIXML\r\n<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\"><Obj S=\"progress\" RefId=\"0\"><TN RefId=\"0\"><T>System.Management.Automation.PSCustomObject</T></TN><MS><I64 N=\"SourceId\">1</I64></MS></Obj></Objs>";

    #[test]
    fn interpret_output_decides_from_exit_code_and_the_sentinel() {
        let ok = out(0, "IGN-RESULT:{\"ok\":1}\r\n", "", false);
        assert_eq!(interpret_output(&ok, 5, None).expect("ok")["ok"], 1);
        // progress CLIXML on stderr does not make a success a failure
        let progress = out(0, "IGN-RESULT:{\"ok\":1}\r\n", CLIXML_PROGRESS, false);
        assert!(interpret_output(&progress, 5, None).is_ok());

        // non-zero exit: failure, with the CLIXML error text decoded for display
        let failed = out(1, "", CLIXML_ERROR, false);
        let detail = interpret_output(&failed, 5, None).expect_err("exit 1");
        assert!(detail.starts_with("exit code 1: "), "{detail}");
        assert!(detail.contains("Cannot find path"), "{detail}");
        assert!(
            !detail.contains("<S S="),
            "no raw XML in the detail: {detail}"
        );
        // exit 0 with an error record on stderr
        let recorded = out(0, "IGN-RESULT:{\"ok\":1}\r\n", CLIXML_ERROR, false);
        let detail = interpret_output(&recorded, 5, None).expect_err("error record");
        assert!(detail.contains("error record"), "{detail}");
        // a missing sentinel with exit 0
        let silent = out(0, "all good\r\n", "", false);
        assert!(interpret_output(&silent, 5, None).is_err());
        // a timeout
        let timed = out(-1, "", "", true);
        assert_eq!(
            interpret_output(&timed, 7, None).expect_err("timeout"),
            "timed out after 7s and was killed"
        );
        // the host refusing a long command line (C7a-7)
        let long = out(1, "", "The command line is too long.\r\n", false);
        let detail = interpret_output(&long, 5, None).expect_err("too long");
        assert!(detail.contains("too long"), "{detail}");
        // ssh itself failing
        let ssh = out(255, "", "exec request failed on channel 0", false);
        assert!(
            interpret_output(&ssh, 5, None)
                .expect_err("255")
                .contains("exit code 255")
        );
    }

    #[test]
    fn clixml_errors_decode_and_ignore_non_clixml() {
        let got = clixml_errors(CLIXML_ERROR.as_bytes());
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            "Cannot find path 'C:\\x' because it does not exist."
        );
        assert!(clixml_errors(b"plain stderr").is_empty());
        assert!(clixml_errors(CLIXML_PROGRESS.as_bytes()).is_empty());
    }

    #[test]
    fn parse_enumeration_cases() {
        let value = serde_json::json!({
            "ok": 1,
            "res": [
                { "i": 0, "x": 1, "e": [["a\\b.json", 0, 12, 0, 90], ["a", 1, 0, 0, 80]] },
                { "i": 1, "x": 0, "e": [] }
            ]
        });
        let got = parse_enumeration(&value, 2).expect("parse");
        let first = got[0].as_ref().expect("root 0 exists");
        assert_eq!(first[0].rel, "a/b.json", "backslashes become slashes");
        assert!(!first[0].dir && first[0].size == 12 && !first[0].reparse);
        assert!(first[1].dir);
        assert!(got[1].is_none(), "a missing root");
        for bad in [
            serde_json::json!({ "ok": 0, "res": [] }),
            serde_json::json!({ "ok": 1 }),
            serde_json::json!({ "ok": 1, "res": [] }),
            serde_json::json!({ "ok": 1, "res": [{ "i": 5, "x": 1, "e": [] }, { "i": 1, "x": 1, "e": [] }] }),
            serde_json::json!({ "ok": 1, "res": [{ "i": 0, "x": 1, "e": [["a"]] }, { "i": 1, "x": 0 }] }),
        ] {
            assert!(parse_enumeration(&bad, 2).is_err(), "{bad}");
        }
    }

    /// The PowerShell the tests can run: `pwsh` on the dev Mac and CI,
    /// `powershell` on Windows. Fails loudly, never skips.
    fn powershell() -> std::path::PathBuf {
        for name in ["pwsh", "powershell", "powershell.exe"] {
            let Some(paths) = std::env::var_os("PATH") else {
                continue;
            };
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        for fixed in [
            "/usr/local/bin/pwsh",
            "/opt/homebrew/bin/pwsh",
            "/usr/bin/pwsh",
        ] {
            if std::path::Path::new(fixed).is_file() {
                return fixed.into();
            }
        }
        panic!(
            "no pwsh or powershell on PATH: install PowerShell 7 (macOS: `brew install --cask powershell`; Debian/Ubuntu: the Microsoft packages) so the script-logic tests can run. These tests are never skipped."
        );
    }

    /// Run `script` for real: the same text `-EncodedCommand` carries, as
    /// `-Command`. Returns the output.
    fn run_script(script: &WinScript) -> std::process::Output {
        let command = encoded_command(script, COMMAND_BUDGET).expect("command");
        let encoded = command
            .as_str()
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .expect("prefix");
        std::process::Command::new(powershell())
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", encoded])
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run powershell")
    }

    #[test]
    fn winps_script_logic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src file.txt");
        let dst = dir.path().join("dst; echo 'hi' $(x).txt");
        std::fs::write(&src, b"new").expect("src");
        std::fs::write(&dst, b"old").expect("dst");

        // MOVE_REPLACE replaces an existing target and prints the sentinel
        let moved = run_script(&WinScript::move_replace(
            src.to_str().expect("utf8"),
            dst.to_str().expect("utf8"),
        ));
        assert!(moved.status.success(), "{moved:?}");
        let value = parse_ign_result(&moved.stdout).expect("sentinel");
        assert_eq!(value["ok"], 1);
        assert_eq!(std::fs::read(&dst).expect("dst"), b"new");
        assert!(!src.exists(), "the source was moved");

        // a missing source exits non-zero and prints no sentinel
        let missing = run_script(&WinScript::move_replace(
            dir.path().join("nope").to_str().expect("utf8"),
            dst.to_str().expect("utf8"),
        ));
        assert!(!missing.status.success(), "a missing source must fail");
        assert!(parse_ign_result(&missing.stdout).is_err());

        // a script that exits 7 propagates its code
        let seven = run_script(&WinScript {
            name: "seven",
            body: "exit 7",
            params: Value::Null,
        });
        assert_eq!(seven.status.code(), Some(7));

        // non-ASCII text leaves the host as \u escapes, ASCII only
        let unicode = run_script(&WinScript {
            name: "unicode",
            body: "E @{s=\"caf\u{e9} \u{1f600}\"}",
            params: Value::Null,
        });
        assert!(unicode.status.success(), "{unicode:?}");
        assert!(unicode.stdout.is_ascii(), "stdout must be ASCII");
        let value = parse_ign_result(&unicode.stdout).expect("sentinel");
        assert_eq!(value["s"], "caf\u{e9} \u{1f600}");
    }

    #[test]
    fn winps_enumerate_reports_files_dirs_and_reparse_points() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("a b").join("c")).expect("dirs");
        std::fs::write(root.join("top.json"), b"12345").expect("file");
        std::fs::write(root.join("a b").join("caf\u{e9}.json"), b"xyz").expect("file");
        std::fs::write(root.join("a b").join("c").join("deep.json"), b"").expect("file");
        let absent = dir.path().join("absent");
        let run = |roots: &[String]| {
            let out = run_script(&WinScript::enumerate("", roots));
            assert!(out.status.success(), "{out:?}");
            parse_enumeration(
                &parse_ign_result(&out.stdout).expect("sentinel"),
                roots.len(),
            )
            .expect("enumeration")
        };
        let roots = vec![
            root.to_str().expect("utf8").to_string(),
            absent.to_str().expect("utf8").to_string(),
        ];
        let got = run(&roots);
        let mut entries: Vec<(String, bool, u64, bool)> = got[0]
            .as_ref()
            .expect("root exists")
            .iter()
            .map(|e| (e.rel.clone(), e.dir, e.size, e.reparse))
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            [
                ("a b".to_string(), true, 0, false),
                ("a b/c".to_string(), true, 0, false),
                ("a b/c/deep.json".to_string(), false, 0, false),
                ("a b/caf\u{e9}.json".to_string(), false, 3, false),
                ("top.json".to_string(), false, 5, false),
            ]
        );
        assert!(got[1].is_none(), "a missing root is reported as missing");

        // an empty root exists and has no entries
        let empty = dir.path().join("empty");
        std::fs::create_dir(&empty).expect("empty");
        let got = run(&[empty.to_str().expect("utf8").to_string()]);
        assert_eq!(got[0].as_ref().expect("exists").len(), 0);

        // a symlink is a reparse point on every platform pwsh runs on
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("link")).expect("symlink");
            let got = run(&[root.to_str().expect("utf8").to_string()]);
            let link = got[0]
                .as_ref()
                .expect("exists")
                .iter()
                .find(|e| e.rel == "link")
                .expect("the link is reported");
            // on macOS/Linux pwsh the ReparsePoint attribute is set for symlinks
            assert!(link.reparse, "a symlink is flagged: {link:?}");
        }
    }

    /// A resources tree for the BUNDLE tests: names with a space, a comma and
    /// a non-ASCII letter, nested folders, an empty file.
    fn bundle_tree(
        dir: &std::path::Path,
    ) -> (std::path::PathBuf, Vec<super::super::bundle::BundleFile>) {
        use super::super::bundle::BundleFile;
        let resources = dir.join("config").join("resources");
        let files = [
            ("core", "ignition/a b,1/config.json", &b"{\"a\":1}"[..]),
            ("core", "ignition/caf\u{e9}/x.json", b""),
            (
                "production",
                "ignition/deep/er/f.json",
                b"{\"deep\":[1,2,3]}",
            ),
        ];
        let mut listed = Vec::new();
        for (collection, rel, body) in files {
            let path = rel
                .split('/')
                .fold(resources.join(collection), |p, c| p.join(c));
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(&path, body).expect("file");
            listed.push(BundleFile {
                collection: collection.to_string(),
                rel: rel.to_string(),
                size: body.len() as u64,
            });
        }
        (resources, listed)
    }

    /// Run BUNDLE over `files` below `resources` with the list in `stage`
    /// (the list is written and hashed here unless `sha` overrides it).
    fn run_bundle(
        resources: &std::path::Path,
        stage: &std::path::Path,
        files: &[super::super::bundle::BundleFile],
        sha: Option<&str>,
        need: u64,
    ) -> std::process::Output {
        let list = super::super::bundle::render_list(files);
        let list_path = stage.join("read.tsv");
        std::fs::write(&list_path, &list).expect("list");
        let digest = crate::deploy::tree::sha256_hex(&list);
        run_script(&WinScript::bundle(
            "",
            resources.to_str().expect("utf8"),
            list_path.to_str().expect("utf8"),
            stage.join("read.zip").to_str().expect("utf8"),
            sha.unwrap_or(&digest),
            need,
        ))
    }

    /// Quick 261105-dl1: BUNDLE zips exactly the listed files, removes the
    /// list, and on every refusal leaves neither the list nor an archive.
    #[test]
    fn winps_bundle_zips_exactly_the_listed_files() {
        use super::super::bundle;
        let dir = tempfile::tempdir().expect("tempdir");
        let (resources, files) = bundle_tree(dir.path());
        // an unlisted file next to a listed one is never bundled
        std::fs::write(
            resources
                .join("core")
                .join("ignition")
                .join("unlisted.json"),
            b"no",
        )
        .expect("unlisted");
        let stage = dir.path().join("stage");
        std::fs::create_dir(&stage).expect("stage");
        let archive = stage.join("read.zip");
        let left = |stage: &std::path::Path| -> Vec<String> {
            std::fs::read_dir(stage)
                .expect("stage")
                .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .collect()
        };

        // (a) an honest bundle round-trips
        let out = run_bundle(&resources, &stage, &files, None, 1);
        assert!(out.status.success(), "{out:?}");
        let value = parse_ign_result(&out.stdout).expect("sentinel");
        assert_eq!(value["ok"], 1, "{value}");
        assert_eq!(value["n"], 3, "{value}");
        let z = std::fs::metadata(&archive).expect("archive").len();
        assert_eq!(value["z"].as_u64(), Some(z), "{value}");
        assert!(z <= bundle::archive_limit(&files));
        assert_eq!(left(&stage), ["read.zip"], "the list is removed");
        let attributes = bundle::record_attributes(&archive).expect("attributes");
        eprintln!("pwsh BUNDLE records (name, made-by host, external attributes): {attributes:?}");
        let out_dir = dir.path().join("out");
        assert_eq!(
            bundle::extract(&archive, &files, &out_dir, 0).expect("extract"),
            files.iter().map(|f| f.size).sum::<u64>()
        );
        for f in &files {
            let rel = f
                .rel
                .split('/')
                .fold(out_dir.join(&f.collection), |p, c| p.join(c));
            let src = f
                .rel
                .split('/')
                .fold(resources.join(&f.collection), |p, c| p.join(c));
            assert_eq!(
                std::fs::read(rel).expect("out"),
                std::fs::read(src).expect("src")
            );
        }
        std::fs::remove_file(&archive).expect("rm");

        // (b) a list whose hash does not match
        let out = run_bundle(&resources, &stage, &files, Some(&"0".repeat(64)), 1);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(
            parse_ign_result(&out.stdout).expect("sentinel"),
            serde_json::json!({ "ok": 0, "err": "list" })
        );
        assert!(left(&stage).is_empty(), "{:?}", left(&stage));

        // (c) a listed size that is not the file's
        let mut changed = files.clone();
        changed[2].size += 1;
        let out = run_bundle(&resources, &stage, &changed, None, 1);
        assert!(out.status.success(), "{out:?}");
        let value = parse_ign_result(&out.stdout).expect("sentinel");
        assert_eq!(
            value,
            serde_json::json!({ "ok": 0, "err": "changed", "i": 2, "n": files[2].size })
        );
        assert!(left(&stage).is_empty(), "{:?}", left(&stage));

        // (d) a listed file that is gone
        let mut missing = files.clone();
        missing[1].rel = "ignition/caf\u{e9}/gone.json".to_string();
        let out = run_bundle(&resources, &stage, &missing, None, 1);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(
            parse_ign_result(&out.stdout).expect("sentinel"),
            serde_json::json!({ "ok": 0, "err": "missing", "i": 1 })
        );
        assert!(left(&stage).is_empty(), "{:?}", left(&stage));

        // (e) an archive path that exists is never written or removed
        std::fs::write(&archive, b"keep me").expect("existing");
        let out = run_bundle(&resources, &stage, &files, None, 1);
        assert!(!out.status.success(), "CreateNew must fail: {out:?}");
        assert!(parse_ign_result(&out.stdout).is_err());
        assert_eq!(std::fs::read(&archive).expect("kept"), b"keep me");
        assert_eq!(left(&stage), ["read.zip"], "the list is removed");
        std::fs::remove_file(&archive).expect("rm");

        // (f) too little free space
        let out = run_bundle(&resources, &stage, &files, None, u64::MAX / 4);
        assert!(out.status.success(), "{out:?}");
        let value = parse_ign_result(&out.stdout).expect("sentinel");
        assert_eq!(value["ok"], 0, "{value}");
        assert_eq!(value["err"], "space", "{value}");
        assert!(value["free"].as_u64().is_some(), "{value}");
        assert!(left(&stage).is_empty(), "{:?}", left(&stage));
    }

    /// Quick 261105-cr1: BUNDLE stops copying a file one byte past its
    /// listed size, inside the read loop, so a file that keeps growing
    /// cannot grow the archive. It answers `changed` with the bytes it
    /// copied, and leaves neither the list nor an archive.
    #[test]
    fn winps_bundle_stops_a_file_past_its_listed_size() {
        use super::super::bundle::BundleFile;
        let dir = tempfile::tempdir().expect("tempdir");
        let resources = dir.path().join("config").join("resources");
        let big = resources.join("core").join("ignition").join("big.json");
        std::fs::create_dir_all(big.parent().expect("parent")).expect("dirs");
        std::fs::write(&big, vec![b'x'; 300_000]).expect("big");
        let small = resources.join("core").join("ignition").join("small.json");
        std::fs::write(&small, b"{\"deep\":[1,2,3]}").expect("small");
        let stage = dir.path().join("stage");
        std::fs::create_dir(&stage).expect("stage");
        for (rel, listed) in [("ignition/big.json", 70_000u64), ("ignition/small.json", 3)] {
            let files = [BundleFile {
                collection: "core".to_string(),
                rel: rel.to_string(),
                size: listed,
            }];
            let out = run_bundle(&resources, &stage, &files, None, 1);
            assert!(out.status.success(), "{out:?}");
            assert_eq!(
                parse_ign_result(&out.stdout).expect("sentinel"),
                serde_json::json!({ "ok": 0, "err": "changed", "i": 0, "n": listed + 1 }),
                "{rel}"
            );
            assert_eq!(
                std::fs::read_dir(&stage).expect("stage").count(),
                0,
                "{rel}: neither the list nor an archive is left"
            );
        }
    }

    /// Quick 261105-dl1: BUNDLE_CLEAN removes the named files and the
    /// staging dir, and touches nothing else.
    #[test]
    fn winps_bundle_clean_removes_only_the_staging_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let clean = |d: &std::path::Path| {
            let out = run_script(&WinScript::bundle_clean(
                "",
                d.to_str().expect("utf8"),
                &["read.tsv", "read.zip"],
            ));
            assert!(out.status.success(), "{out:?}");
            parse_ign_result(&out.stdout).expect("sentinel")
        };
        let stage = dir.path().join(".ign-ctl-1");
        std::fs::create_dir(&stage).expect("stage");
        std::fs::write(stage.join("read.tsv"), b"l").expect("list");
        std::fs::write(stage.join("read.zip"), b"z").expect("zip");
        assert_eq!(clean(&stage), serde_json::json!({ "ok": 1 }));
        assert!(!stage.exists(), "the staging dir is removed");
        // a missing dir is clean already
        assert_eq!(clean(&stage), serde_json::json!({ "ok": 1 }));

        // anything else stays, and so does the dir
        std::fs::create_dir(&stage).expect("stage");
        std::fs::write(stage.join("read.zip"), b"z").expect("zip");
        std::fs::write(stage.join("other.txt"), b"mine").expect("other");
        assert_eq!(clean(&stage), serde_json::json!({ "ok": 0, "err": "left" }));
        assert_eq!(
            std::fs::read(stage.join("other.txt")).expect("other"),
            b"mine"
        );
        assert!(!stage.join("read.zip").exists());

        // a staging dir that is a symlink is not followed
        #[cfg(unix)]
        {
            let target = dir.path().join("elsewhere");
            std::fs::create_dir(&target).expect("target");
            std::fs::write(target.join("read.tsv"), b"keep").expect("keep");
            let link = dir.path().join(".ign-ctl-2");
            std::os::unix::fs::symlink(&target, &link).expect("symlink");
            assert_eq!(
                clean(&link),
                serde_json::json!({ "ok": 0, "err": "reparse" })
            );
            assert_eq!(
                std::fs::read(target.join("read.tsv")).expect("kept"),
                b"keep"
            );
        }
    }

    /// Windows PowerShell 5.1 (.NET Framework) has no
    /// `Split(char, StringSplitOptions)`: BUNDLE must split its list with the
    /// `char[]` overload both runtimes have, which local pwsh 7 cannot tell
    /// apart (found live, quick 261105-dl1).
    #[test]
    fn bundle_splits_its_list_with_an_overload_ps51_has() {
        assert!(
            BUNDLE.contains(".Split([char[]]@([char]10),[StringSplitOptions]::RemoveEmptyEntries)"),
            "BUNDLE must use Split(char[], StringSplitOptions)"
        );
        assert!(!BUNDLE.contains(".Split([char]10,"));
    }

    /// Both bundle commands fit the command budget with 150-character native
    /// paths.
    #[test]
    fn bundle_commands_fit_the_budget() {
        let long = |tail: &str| {
            let mut path = String::from("C:\\");
            while path.len() + tail.len() + 1 < 150 {
                path.push_str("Program Files x\\");
            }
            path.truncate(150 - tail.len() - 1);
            format!("{path}\\{tail}")
        };
        let bundle = WinScript::bundle(
            LONG_PATH_PREFIX,
            &long("resources"),
            &long("read.tsv"),
            &long("read.zip"),
            &"a".repeat(64),
            u64::MAX,
        );
        let clean =
            WinScript::bundle_clean(LONG_PATH_PREFIX, &long("ctl"), &["read.tsv", "read.zip"]);
        for script in [bundle, clean] {
            let command =
                encoded_command(&script, COMMAND_BUDGET).unwrap_or_else(|why| panic!("{why}"));
            eprintln!(
                "{}: {} characters",
                script.name,
                command.as_str().chars().count()
            );
        }
    }

    /// C6 (PR #31 review): the names-only listing reports the direct
    /// children of each root, with the same columns as [`ENUMERATE`], and
    /// never opens a child directory.
    #[test]
    fn winps_enumerate_children_reports_direct_children_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("a b").join("c")).expect("dirs");
        std::fs::write(root.join("top.json"), b"12345").expect("file");
        std::fs::write(root.join("a b").join("x.json"), b"xyz").expect("file");
        std::fs::write(root.join("a b").join("c").join("deep.json"), b"").expect("file");
        let absent = dir.path().join("absent");
        let roots = vec![
            root.to_str().expect("utf8").to_string(),
            absent.to_str().expect("utf8").to_string(),
        ];
        let run = |roots: &[String]| {
            let out = run_script(&WinScript::enumerate_children("", roots));
            assert!(out.status.success(), "{out:?}");
            parse_enumeration(
                &parse_ign_result(&out.stdout).expect("sentinel"),
                roots.len(),
            )
            .expect("enumeration")
        };
        let got = run(&roots);
        let listed = got[0].as_ref().expect("root exists");
        let mut entries: Vec<(String, bool, u64, bool)> = listed
            .iter()
            .map(|e| (e.rel.clone(), e.dir, e.size, e.reparse))
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            [
                ("a b".to_string(), true, 0, false),
                ("top.json".to_string(), false, 5, false),
            ]
        );
        let top = listed.iter().find(|e| e.rel == "top.json").expect("top");
        assert!(top.full_len >= root.to_str().expect("utf8").len() + "/top.json".len());
        assert!(got[1].is_none(), "a missing root is reported as missing");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("a b"), root.join("link")).expect("symlink");
            let got = run(&roots[..1]);
            let listed = got[0].as_ref().expect("exists");
            let link = listed
                .iter()
                .find(|e| e.rel == "link")
                .expect("the link is reported");
            assert!(link.reparse, "a symlink is flagged: {link:?}");
            assert!(
                listed.iter().all(|e| !e.rel.contains('/')),
                "a reparse point is not followed: {listed:?}"
            );
        }
    }

    /// C6: the names-only body has no walk (no stack, no push), is one line
    /// of printable ASCII and fits the budget with a realistic root; the
    /// read's [`ENUMERATE`] keeps its walk.
    #[test]
    fn enumerate_children_never_descends() {
        let body = ENUMERATE_CHILDREN;
        assert!(body.bytes().all(|b| (0x20..=0x7e).contains(&b)));
        assert!(!body.contains("Stack") && !body.contains("Push"), "{body}");
        assert!(ENUMERATE.contains("Collections.Stack") && ENUMERATE.contains(".Push("));
        let root = "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-provider".to_string();
        assert!(
            encoded_command(
                &WinScript::enumerate_children(LONG_PATH_PREFIX, &[root]),
                COMMAND_BUDGET
            )
            .is_ok()
        );
    }

    /// G-22-1: the relative paths come from the walk, not from cutting the
    /// caller's spelling of the root off each `FullName`. .NET rewrites a root
    /// it normalises: it expands an 8.3 component (`RUNNER~1` becomes
    /// `runneradmin` under the Windows runner's `%TEMP%`, 3 characters longer)
    /// and drops `.`, doubled separators and a trailing separator, so a length
    /// taken from the caller's string shifts every entry (`ot\a b` for `a b`).
    #[test]
    fn winps_enumerate_rel_paths_do_not_depend_on_the_root_spelling() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("a b").join("c")).expect("dirs");
        std::fs::write(root.join("top.json"), b"12345").expect("file");
        std::fs::write(root.join("a b").join("c").join("deep.json"), b"").expect("file");
        let base = dir.path().to_str().expect("utf8");
        let sep = std::path::MAIN_SEPARATOR;
        let expected = [
            ("a b".to_string(), true, 0, false),
            ("a b/c".to_string(), true, 0, false),
            ("a b/c/deep.json".to_string(), false, 0, false),
            ("top.json".to_string(), false, 5, false),
        ];
        for spelling in [
            // on the Windows runner this one is the 8.3 `RUNNER~1` path
            format!("{base}{sep}root"),
            format!("{base}{sep}.{sep}root"),
            format!("{base}{sep}{sep}root"),
            format!("{base}{sep}root{sep}"),
        ] {
            let roots = [spelling.clone()];
            let out = run_script(&WinScript::enumerate("", &roots));
            assert!(out.status.success(), "{spelling}: {out:?}");
            let got = parse_enumeration(&parse_ign_result(&out.stdout).expect("sentinel"), 1)
                .expect("enumeration");
            let mut entries: Vec<(String, bool, u64, bool)> = got[0]
                .as_ref()
                .expect("root exists")
                .iter()
                .map(|e| (e.rel.clone(), e.dir, e.size, e.reparse))
                .collect();
            entries.sort();
            assert_eq!(entries, expected, "root spelled {spelling:?}");
        }
    }

    /// Gate parity (post-review): everything the POSIX gate refuses, and the
    /// Windows-only ways to escape a directory, are refused as a component.
    #[test]
    fn windows_component_gate_refuses_what_posix_refuses_and_more() {
        for ok in [
            "plain",
            "MES db,1",
            "Caf\u{e9} DB",
            ".resources",
            "Program Files (x86)",
            "a.b",
            "%43%4f%4e",
            "CONSOLE",
            "NULL1",
            "COM10",
        ] {
            assert_eq!(windows_component_problem(ok), None, "{ok:?} is allowed");
        }
        for bad in [
            // the POSIX gate
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a\0b",
            "a\nb",
            // drive letters and alternate data streams
            "C:",
            "C:x",
            "a:b",
            "name:stream",
            "name::$DATA",
            // \\?\ and UNC prefixes arrive as separators and colons
            "\\\\?\\C:",
            "?",
            // Windows strips a trailing dot or space: two names, one entry
            "a.",
            "a ",
            "a..",
            // reserved device names, with or without an extension
            "CON",
            "con",
            "Nul",
            "aux.txt",
            "PRN.json",
            "COM1",
            "lpt9.cfg",
            // characters Windows forbids
            "a<b",
            "a>b",
            "a|b",
            "a\"b",
            "a*b",
        ] {
            assert!(
                windows_component_problem(bad).is_some(),
                "{bad:?} must be refused"
            );
        }
    }

    #[test]
    fn windows_absolute_path_gate() {
        for ok in [
            "/C:/Users/svc",
            "/c:/Users/svc/ign-deploy/gw/production/staging/run-1",
            "/D:/a b/c",
        ] {
            assert_eq!(windows_absolute_path_problem(ok), None, "{ok}");
        }
        for bad in [
            "",
            "/",
            "/C:",
            "C:/Users/svc",
            "/Users/svc",
            "/C:/Users/../x",
            "/C:/Users/./x",
            "/C:/Users//x",
            "/C:/Users/svc/D:/x",
            "/C:/Users/svc/x:stream",
            "/C:/Users/svc/x.",
            "/C:/Users/svc/NUL",
            "//server/share/x",
            "/C:\\Users",
            "/C:/Users/svc\0",
            "/\\\\?\\C:/x",
            "/CC:/x",
        ] {
            assert!(
                windows_absolute_path_problem(bad).is_some(),
                "{bad:?} must be refused"
            );
        }
    }

    #[test]
    fn parse_enumeration_applies_the_windows_gate_to_every_path() {
        let entry = |rel: &str| {
            serde_json::json!({
                "ok": 1,
                "res": [{ "i": 0, "x": 1, "e": [[rel, 0, 1, 0, 50]] }]
            })
        };
        assert!(parse_enumeration(&entry("a\\b.json"), 1).is_ok());
        for bad in [
            "..\\x",
            "a\\..\\b",
            "\\\\?\\C:\\x",
            "C:\\x",
            "/abs",
            "a\\\\b",
            "a:b\\c",
            "a\\b.",
            "a\\NUL",
            "",
            "a\0b",
        ] {
            assert!(
                parse_enumeration(&entry(bad), 1).is_err(),
                "{bad:?} must be refused"
            );
        }
    }

    // ---- the script stub (23-08) --------------------------------------------

    fn roots(staging: &str, collection: &str, deploy: &str) -> StubRoots {
        StubRoots {
            staging: staging.to_string(),
            collection: collection.to_string(),
            deploy: deploy.to_string(),
        }
    }

    fn sha(fill: char) -> String {
        std::iter::repeat_n(fill, 64).collect()
    }

    #[test]
    fn stub_fits_the_budget() {
        // the longest roots a legal node can make: the data directory is held to
        // WINDOWS_DATA_DIR_MAX (150) characters and everything below the account
        // home stays under LONG_PATH_LIMIT; 259-character roots are past both
        for len in [60usize, 150, 240, 259] {
            let root = |tag: char| format!("C:\\{}", tag.to_string().repeat(len - 3));
            let stub = script_stub(
                &sha('a'),
                &sha('b'),
                "restore",
                &roots(&root('s'), &root('c'), &root('d')),
            );
            let command = encoded_command(&stub, COMMAND_BUDGET)
                .unwrap_or_else(|why| panic!("{len}-character roots: {why}"));
            eprintln!(
                "stub with {len}-character roots: {} characters",
                command.as_str().chars().count()
            );
            assert!(
                command.as_str().chars().count() <= COMMAND_BUDGET,
                "{len}-character roots"
            );
        }
        // non-ASCII path characters travel as UTF-8 inside the JSON and still fit
        let accented = format!("C:\\Users\\{}", "\u{e9}".repeat(100));
        let stub = script_stub(
            &sha('a'),
            &sha('b'),
            "commit",
            &roots(&accented, &accented, &accented),
        );
        assert!(encoded_command(&stub, COMMAND_BUDGET).is_ok());
    }

    #[test]
    fn stub_body_is_fixed_text_and_carries_no_path_or_hash() {
        let stub = script_stub(
            &sha('a'),
            &sha('b'),
            "commit",
            &roots(
                "C:\\Users\\svc\\ign-deploy\\gw\\production\\staging\\run-1",
                "C:\\Data\\config\\resources\\production",
                "C:\\Users\\svc\\ign-deploy\\gw\\production",
            ),
        );
        assert_eq!(stub.body, STUB);
        // the body is one line of printable ASCII
        assert!(STUB.bytes().all(|b| (0x20..0x7f).contains(&b)), "{STUB}");
        assert!(!STUB.contains("ign-deploy") && !STUB.contains("Users"));
        // the parameters hold the roots, the operation and both hashes
        let text = decode_command(&encoded_command(&stub, COMMAND_BUDGET).expect("command"));
        let blob = text
            .split("FromBase64String('")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .expect("blob");
        let json = base64::engine::general_purpose::STANDARD
            .decode(blob)
            .expect("blob base64");
        let value: Value = serde_json::from_slice(&json).expect("json");
        assert_eq!(value["o"], "commit");
        assert_eq!(value["ss"], sha('a'));
        assert_eq!(value["ws"], sha('b'));
        assert_eq!(
            value["t"],
            "C:\\Users\\svc\\ign-deploy\\gw\\production\\staging\\run-1"
        );
        // the command text itself shows no path (the parameters are base64)
        assert!(!text.contains("staging"));
        assert!(text.ends_with(STUB));
    }

    #[test]
    fn stub_verifies_before_it_runs_and_hashes_the_bytes_it_runs() {
        // exit 97 comes before the script block is created, and the script and
        // the work list are each read once, hashed and then used from memory
        let verify = STUB.find("exit 97").expect("the mismatch exit");
        let run = STUB.find("[ScriptBlock]::Create").expect("the run");
        assert!(verify < run);
        assert_eq!(STUB.matches("ReadAllBytes").count(), 2);
        assert!(STUB.contains("GetString($b)") && STUB.contains("GetString($w)"));
        assert!(!STUB.contains("ReadAllText"));
        assert!(!STUB.contains("-File") && !STUB.contains("Invoke-Expression"));
    }

    #[test]
    fn the_shipped_script_is_ascii_text_the_stub_can_hash() {
        use crate::deploy::transport::batch::{SCRIPT_PS1, script_ps1_sha256};
        assert!(SCRIPT_PS1.is_ascii(), "the script is pure ASCII");
        assert!(!SCRIPT_PS1.starts_with('\u{feff}'), "no byte order mark");
        assert!(SCRIPT_PS1.contains("IGN-STOP") || SCRIPT_PS1.contains("E $S.Res"));
        assert_eq!(
            script_ps1_sha256(),
            crate::deploy::tree::sha256_hex(SCRIPT_PS1.as_bytes())
        );
        assert_eq!(script_ps1_sha256().len(), 64);
        // it starts with the parameter block after its comment header and uses
        // none of the things a script block run by the stub cannot rely on
        assert!(SCRIPT_PS1.contains("\nparam([string]$Op"));
        assert!(!SCRIPT_PS1.contains("$MyInvocation") && !SCRIPT_PS1.contains("$PSScriptRoot"));
    }

    /// Windows PowerShell 5.1 resolves an alias before a function, so a script
    /// function named like an alias is never called (`LP`, the alias of
    /// `Out-Printer`, made every hash of the first live run empty). The list is
    /// `Get-Alias` on the capture host (Windows PowerShell 5.1.20348, 2026-10-17).
    const ALIASES_5_1: &str = "% ? ac asnp cat cd cfs chdir clc clear clhy cli clp cls clv cnsn compare copy cp cpi cpp curl cvpa dbp del diff dir dnsn ebp echo epal epcsv epsn erase etsn exsn fc fhx fl foreach ft fw gal gbp gc gci gcm gcs gdr ghy gi gjb gl gm gmo gp gps gpv group gsn gsnp gsv gu gv gwmi h history icm iex ihy ii ipal ipcsv ipmo ipsn irm ise iwmi iwr kill lp ls man md measure mi mount move mp mv nal ndr ni nmo npssc nsn nv ogv oh popd ps pushd pwd r rbp rcjb rcsn rd rdr ren ri rjb rm rmdir rmo rni rnp rp rsn rsnp rujb rv rvpa rwmi sajb sal saps sasv sbp sc select set shcm si sl sleep sls sort sp spjb spps spsv start sujb sv swmi tee trcm type wget where wjb write";

    #[test]
    fn the_shipped_script_names_no_function_like_an_alias_or_a_cmdlet() {
        use crate::deploy::transport::batch::SCRIPT_PS1;
        let aliases: std::collections::BTreeSet<&str> = ALIASES_5_1.split(' ').collect();
        let mut seen = 0;
        for line in SCRIPT_PS1.lines() {
            let Some(rest) = line.strip_prefix("function ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            seen += 1;
            assert!(
                !aliases.contains(name.to_ascii_lowercase().as_str()),
                "function {name} has the name of a Windows PowerShell alias, which wins over it"
            );
            assert!(!name.contains('-'), "function {name} reads like a cmdlet");
        }
        assert!(
            seen > 20,
            "the script defines its helpers as functions ({seen})"
        );
    }

    /// 23-20: the PowerShell script has every operation the POSIX script has,
    /// in the same dispatch, and the 23-08 hardening holds in the new ones: a
    /// file is moved only by `MoveReplace` (so `mirror` reuses `MoveOne` and
    /// gets the access-list reset), a tree is removed with .NET
    /// `Directory.Delete` and never with `Remove-Item -Recurse`, which in Windows
    /// PowerShell 5.1 can follow a junction.
    #[test]
    fn the_shipped_script_runs_every_operation_commit_sh_has_and_keeps_the_hardening() {
        use crate::deploy::transport::batch::{SCRIPT_PS1, SCRIPT_SH};
        let ops = [
            "commit",
            "restore",
            "mirror",
            "fingerprint",
            "probe",
            "finalize",
            "discard",
            "verify",
            "unstage",
        ];
        let mut sh_order: Vec<usize> = Vec::new();
        let mut ps_order: Vec<usize> = Vec::new();
        for op in ops {
            sh_order.push(
                SCRIPT_SH
                    .find(&format!("  {op}) op_{op} ;;"))
                    .unwrap_or_else(|| panic!("commit.sh has no {op}")),
            );
            let ps = SCRIPT_PS1
                .find(&format!("        '{op}' {{ "))
                .unwrap_or_else(|| panic!("commit.ps1 has no {op}"));
            ps_order.push(ps);
        }
        let ranks = |v: &[usize]| {
            let mut idx: Vec<usize> = (0..v.len()).collect();
            idx.sort_by_key(|i| v[*i]);
            idx
        };
        assert_eq!(
            ranks(&sh_order),
            ranks(&ps_order),
            "the same dispatch order"
        );
        let code: Vec<&str> = SCRIPT_PS1
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect();
        let uses = |needle: &str| code.iter().filter(|l| l.contains(needle)).count();
        assert_eq!(uses("Move-Item"), 1, "only MoveReplace moves a file");
        assert_eq!(uses("Remove-Item"), 0, "trees go with Directory.Delete");
        assert!(uses("[IO.Directory]::Delete((Xp $pth), $true)") >= 1);
        // mirror moves through MoveOne, which resets the access list
        let mirror = SCRIPT_PS1
            .split("function OpMirror")
            .nth(1)
            .and_then(|rest| rest.split("# ---- main").next())
            .expect("OpMirror");
        assert!(mirror.contains("Moves"), "{mirror}");
        let moves = SCRIPT_PS1
            .split("function Moves")
            .nth(1)
            .and_then(|rest| rest.split("\nfunction ").next())
            .expect("Moves");
        assert!(moves.contains("MoveOne"));
        let move_one = SCRIPT_PS1
            .split("function MoveOne")
            .nth(1)
            .and_then(|rest| rest.split("\nfunction ").next())
            .expect("MoveOne");
        assert!(move_one.contains("MoveWithRetries") && move_one.contains("ResetAcl"));
    }

    /// quick 261102-cef (A2, P7): both scripts derive the core root from the
    /// checked collection root and never create it, accept a `.core/` key only
    /// below the two routed tag types, keep `<module>/<type>` in core (floor
    /// 3: only a bare tag path takes floor 2) and probe the core folders.
    #[test]
    fn both_scripts_name_the_core_root_and_its_floor() {
        use crate::deploy::transport::batch::{SCRIPT_PS1, SCRIPT_SH};
        let body = |script: &str, start: &str| -> String {
            script
                .split(start)
                .nth(1)
                .and_then(|rest| rest.split("\nfunction ").next())
                .and_then(|rest| rest.split("\n}\n").next())
                .unwrap_or_else(|| panic!("no {start}"))
                .to_string()
        };
        // commit.sh
        assert!(SCRIPT_SH.contains("CR=${CO%/*}/core"), "the sh core root");
        let key = body(SCRIPT_SH, "apply_key() {");
        assert!(
            key.contains(".core/ignition/tag-definition/* | .core/ignition/tag-type-definition/*"),
            "{key}"
        );
        assert!(key.contains(".core/*) return 1"), "{key}");
        let rmdir = body(SCRIPT_SH, "rmdir_up() {");
        assert!(rmdir.contains("_floor=3"), "{rmdir}");
        assert!(
            rmdir.contains("ignition/tag-definition/* | ignition/tag-type-definition/*) _floor=2"),
            "only a bare tag key takes floor 2: {rmdir}"
        );
        assert!(
            !SCRIPT_SH.contains("mkdir -- \"$CR\""),
            "core is never created"
        );
        for code in [
            "core_missing",
            "core_symlink",
            "core_not_dir",
            "core_tags_unwritable:",
            "core_tags_unsafe:",
        ] {
            assert!(SCRIPT_SH.contains(code), "sh {code}");
            assert!(SCRIPT_PS1.contains(code), "ps {code}");
        }
        // commit.ps1
        assert!(
            SCRIPT_PS1.contains("$S.Cr = $Co.Substring(0, $Co.LastIndexOf('\\')) + '\\core'"),
            "the ps core root"
        );
        let key = body(SCRIPT_PS1, "function ApplyKey");
        assert!(
            key.contains("'ignition/tag-definition/', [StringComparison]::Ordinal")
                && key.contains("'ignition/tag-type-definition/', [StringComparison]::Ordinal"),
            "{key}"
        );
        let rmdir = body(SCRIPT_PS1, "function RmdirUp");
        assert!(
            rmdir.contains("$floor = 3") && rmdir.contains("$key.StartsWith("),
            "{rmdir}"
        );
        assert!(rmdir.contains("KeyBase $key"), "{rmdir}");
        assert!(
            !SCRIPT_PS1.contains("CreateDirectory((Xp $S.Cr))"),
            "core is never created"
        );
        let move_one = body(SCRIPT_PS1, "function MoveOne");
        assert!(move_one.contains("ResetAcl $base $rel"), "{move_one}");
    }

    /// C6, C7, C9 (quick 261104-7cy): `OpProbe` walks each touched core key's
    /// folders top-down (the candidate loop that tried the type folder before
    /// `core\ignition` is gone), the walk needs CreateDirectories on the
    /// deepest existing folder when a level below is missing, `MoveOne`
    /// refuses a `.core/` key before `MakeParents` when the core root is not a
    /// real directory, and `MakeParents` checks its base before its loop.
    #[test]
    fn ps_core_probe_walks_top_down_and_never_recreates_core() {
        use crate::deploy::transport::batch::SCRIPT_PS1;
        let body = |start: &str| -> String {
            SCRIPT_PS1
                .split(start)
                .nth(1)
                .and_then(|rest| rest.split("\n}\n").next())
                .unwrap_or_else(|| panic!("no {start}"))
                .to_string()
        };
        // C6: the walk
        assert!(
            !SCRIPT_PS1.contains("foreach ($cand in"),
            "the candidate loop is gone"
        );
        let probe = body("function OpProbe {");
        assert!(probe.contains("CoreWalk $ty (KeyRel $p.path)"), "{probe}");
        let walk = body("function CoreWalk(");
        assert!(walk.contains("$rel.Split('/')"), "{walk}");
        assert!(walk.contains("IsLink $a"), "{walk}");
        assert!(walk.contains("CanAddFile $cur"), "{walk}");
        // C7: CreateDirectories (right bit 4) where a level is missing
        let add_dir = body("function CanAddDir(");
        assert!(add_dir.contains("CanAdd $dir 4"), "{add_dir}");
        assert!(body("function CanAddFile(").contains("CanAdd $dir 2"));
        assert!(walk.contains("CanAddDir $last"), "{walk}");
        // C9: core is never recreated in a write
        let move_one = body("function MoveOne(");
        let guard = move_one.find("IsCoreKey $key").expect("a core key check");
        let attr = move_one.find("Attr $S.Cr").expect("a core root check");
        let parents = move_one
            .find("MakeParents $rel $base")
            .expect("MakeParents");
        assert!(guard < parents && attr < parents, "{move_one}");
        let make = body("function MakeParents(");
        let base = make.find("Attr $cur").expect("a base check");
        let lp = make.find("for (").expect("the loop");
        assert!(base < lp, "the base is checked before the loop: {make}");
        assert!(
            !SCRIPT_PS1.contains("CreateDirectory((Xp $S.Cr))"),
            "core is never created"
        );
    }

    /// A tampered script or work list makes the stub exit 97 before the script
    /// runs. This needs Windows PowerShell (the stub reads `\\?\` paths), so it
    /// runs on the windows-latest CI leg; the same property is asserted against
    /// the real host in `wincap_apply_script_run`.
    #[cfg(windows)]
    #[test]
    fn stub_refuses_hash_mismatch() {
        use crate::deploy::tree::sha256_hex;
        let dir = tempfile::tempdir().expect("temp dir");
        let script = "param($o,$t,$c,$d,$w) [Console]::Out.WriteLine('RAN '+$o)\n";
        let work = "ign-work 1\n";
        std::fs::write(dir.path().join("commit.ps1"), script).expect("script");
        std::fs::write(dir.path().join("work.tsv"), work).expect("work");
        let staging = dir.path().to_string_lossy().into_owned();
        let run = |script_sha: &str, work_sha: &str| {
            let stub = script_stub(
                script_sha,
                work_sha,
                "commit",
                &roots(&staging, &staging, &staging),
            );
            let command = encoded_command(&stub, COMMAND_BUDGET).expect("command");
            let text = command.as_str();
            let b64 = text
                .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
                .expect("prefix");
            std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", b64])
                .output()
                .expect("powershell")
        };
        let good = run(&sha256_hex(script.as_bytes()), &sha256_hex(work.as_bytes()));
        assert_eq!(good.status.code(), Some(0), "{good:?}");
        assert!(String::from_utf8_lossy(&good.stdout).contains("RAN commit"));
        for (script_sha, work_sha) in [
            (sha('0'), sha256_hex(work.as_bytes())),
            (sha256_hex(script.as_bytes()), sha('0')),
        ] {
            let bad = run(&script_sha, &work_sha);
            assert_eq!(bad.status.code(), Some(97), "{bad:?}");
            assert!(!String::from_utf8_lossy(&bad.stdout).contains("RAN"));
        }
    }
}

"""Pattern-based sanitizer for the Phase 21 deploy corpus (21-01).

Usage: python3 -I sanitize.py SRC DST

Copies a gateway config tree (SRC) to a new directory (DST), replacing
anything that must not enter a public repository. It finds everything by
pattern; this file holds no real value. It prints per-rule counts only,
never a value. Rules are documented in SANITIZE.md:

  S1 Embedded secret members   -> SANITIZED + 'A' padding to the original length
  S2 Entra tenant/client GUIDs -> 00000000-0000-4000-8000-0000000000NN
  S3 non-reserved IPv4 literal -> 192.0.2.<last octet>
  S4 tenant mail host          -> smtp.example.com
  S5 jar files                 -> 24-byte text stand-in plus 0x00 0x0D 0x0A 0x00 0x0D
  S6 everything else           -> byte-for-byte copy

Substitution is exact textual replacement on the original text, so key
order, escapes, indentation and the missing trailing newline survive.
"""

import json
import os
import re
import sys

MARKER = "SANITIZED"
FAKE_GUID_PREFIX = "00000000-0000-4000-8000-"
FAKE_HOST = "smtp.example.com"
JAR_STAND_IN = b"IGN-DEPLOY-STAND-IN-JAR" + bytes([0x00, 0x0D, 0x0A, 0x00, 0x0D])

STRING_TOKEN = r'"(?:[^"\\]|\\.)*"'
GUID_RE = re.compile(
    r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
)
IPV4_RE = re.compile(r"(?<!\d)(?<!\d\.)(\d{1,3}(?:\.\d{1,3}){3})(?!\d|\.\d)")
# A client/tenant member (key + value) or any other string token, in text order.
MEMBER_OR_STRING_RE = re.compile(
    r'(?P<key>"(?:clientId|tenantId)"\s*:\s*)(?P<val>' + STRING_TOKEN + r")|(?P<str>" + STRING_TOKEN + r")"
)
MAIL_SUFFIXES = (".mail.protection.outlook.com", ".onmicrosoft.com")
LOGIN_NEEDLE = "login.microsoftonline.com/"


class Obj(list):
    """A JSON object as an ordered list of (key, value) pairs."""


def ipv4_allowed(octets):
    a, b, c, d = octets
    return (
        a == 127
        or octets == (0, 0, 0, 0)
        or 224 <= a <= 239
        or (a, b, c) == (192, 0, 2)
        or (a, b, c) == (198, 51, 100)
        or (a, b, c) == (203, 0, 113)
    )


def parse(text):
    return json.loads(text, object_pairs_hook=Obj)


def key_sequence(node):
    """Structural fingerprint: every key in order, with array lengths."""
    out = []

    def walk(n):
        if isinstance(n, Obj):
            for k, v in n:
                out.append(("k", k))
                walk(v)
        elif isinstance(n, list):
            out.append(("a", len(n)))
            for v in n:
                walk(v)

    walk(node)
    return out


def embedded_data_strings(node, acc):
    """Collect string members of every Embedded secret's `data` object."""
    if isinstance(node, Obj):
        pairs = dict(node)
        if pairs.get("type") == "Embedded" and isinstance(pairs.get("data"), Obj):
            for _, v in pairs["data"]:
                if isinstance(v, str):
                    acc.add(v)
        for _, v in node:
            embedded_data_strings(v, acc)
    elif isinstance(node, list):
        for v in node:
            embedded_data_strings(v, acc)


class Sanitizer:
    def __init__(self):
        self.guid_map = {}
        self.s1_files = 0
        self.s3_count = 0
        self.s4_count = 0
        self.s5_count = 0
        self.verbatim = 0

    def fake_guid(self, real):
        key = real.lower()
        if key not in self.guid_map:
            self.guid_map[key] = "%s%012d" % (FAKE_GUID_PREFIX, len(self.guid_map) + 1)
        return self.guid_map[key]

    def sub_guids(self, text):
        return GUID_RE.sub(lambda m: self.fake_guid(m.group(0)), text)

    def sub_ipv4(self, token):
        def repl(m):
            parts = m.group(1).split(".")
            nums = tuple(int(p) for p in parts)
            if any(n > 255 for n in nums) or ipv4_allowed(nums):
                return m.group(0)
            self.s3_count += 1
            return "192.0.2." + parts[3]

        return IPV4_RE.sub(repl, token)

    def string_token(self, token):
        """S2 (login URL), S3 and S4 on one quoted string token."""
        inner = token[1:-1]
        if inner.endswith(MAIL_SUFFIXES):
            self.s4_count += 1
            return '"%s"' % FAKE_HOST
        if LOGIN_NEEDLE in inner:
            token = '"%s"' % self.sub_guids(inner)
        return '"%s"' % self.sub_ipv4(token[1:-1])

    def json_text(self, text, relpath):
        node = parse(text)
        out = text

        # S1: exact quoted tokens of the Embedded data members.
        values = set()
        embedded_data_strings(node, values)
        touched = False
        for value in sorted(values):
            if "\\" in value or '"' in value:
                raise SystemExit("%s: S1 value needs escapes, refusing" % relpath)
            replacement = MARKER + "A" * max(0, len(value) - len(MARKER))
            token = '"%s"' % value
            if token in out:
                out = out.replace(token, '"%s"' % replacement)
                touched = True
        if touched:
            self.s1_files += 1

        # S2/S3/S4 in text order.
        def repl(m):
            if m.group("key") is not None:
                val = m.group("val")
                inner = val[1:-1]
                if GUID_RE.fullmatch(inner):
                    val = '"%s"' % self.fake_guid(inner)
                return m.group("key") + val
            return self.string_token(m.group("str"))

        return MEMBER_OR_STRING_RE.sub(repl, out)


def census(text, relpath):
    """Residual-leak scan over sanitized JSON text. Returns rule names hit."""
    node = parse(text)
    bad = []
    values = set()
    embedded_data_strings(node, values)
    if any(not v.startswith(MARKER) for v in values):
        bad.append("S1")
    strings = []

    def collect(n, key=None):
        if isinstance(n, Obj):
            for k, v in n:
                if k in ("clientId", "tenantId") and isinstance(v, str):
                    for g in GUID_RE.findall(v):
                        if not g.startswith(FAKE_GUID_PREFIX):
                            bad.append("S2")
                collect(v, k)
        elif isinstance(n, list):
            for v in n:
                collect(v, key)
        elif isinstance(n, str):
            strings.append(n)

    collect(node)
    for s in strings:
        if LOGIN_NEEDLE in s:
            for g in GUID_RE.findall(s):
                if not g.startswith(FAKE_GUID_PREFIX):
                    bad.append("S2")
        for m in IPV4_RE.finditer(s):
            nums = tuple(int(p) for p in m.group(1).split("."))
            if all(n <= 255 for n in nums) and not ipv4_allowed(nums):
                bad.append("S3")
        low = s.lower()
        if "mail.protection.outlook.com" in low or "onmicrosoft.com" in low:
            bad.append("S4")
    return sorted(set(bad))


def differing_strings(a, b, path=()):
    """Yield (path, before, after) for every leaf that differs between two trees."""
    if isinstance(a, Obj) and isinstance(b, Obj):
        for (ka, va), (kb, vb) in zip(a, b):
            yield from differing_strings(va, vb, path + (ka,))
    elif isinstance(a, list) and isinstance(b, list):
        for i, (va, vb) in enumerate(zip(a, b)):
            yield from differing_strings(va, vb, path + (i,))
    elif a != b:
        yield (path, a, b)


def rule_explains(path, before, embedded):
    """True when a changed leaf is one an S1-S4 rule is allowed to change."""
    if not isinstance(before, str):
        return False
    return (
        before in embedded
        or (path and path[-1] in ("clientId", "tenantId"))
        or LOGIN_NEEDLE in before
        or IPV4_RE.search(before) is not None
        or before.endswith(MAIL_SUFFIXES)
    )


def main(argv):
    if len(argv) != 3:
        raise SystemExit("usage: python3 -I sanitize.py SRC DST")
    src, dst = argv[1], argv[2]
    if not os.path.isdir(src):
        raise SystemExit("SRC is not a directory")
    if os.path.exists(dst) and os.listdir(dst):
        raise SystemExit("DST exists and is not empty, refusing")

    files = []
    for root, dirs, names in os.walk(src):
        dirs.sort(key=lambda d: d.encode("utf-8"))
        for name in names:
            full = os.path.join(root, name)
            files.append(os.path.relpath(full, src))
    files.sort(key=lambda p: p.replace(os.sep, "/").encode("utf-8"))

    san = Sanitizer()
    failures = []
    for relpath in files:
        source = os.path.join(src, relpath)
        target = os.path.join(dst, relpath)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        if relpath.endswith(".jar"):
            data = JAR_STAND_IN
            san.s5_count += 1
        elif relpath.endswith(".json"):
            with open(source, "rb") as f:
                raw = f.read()
            text = raw.decode("utf-8")
            new_text = san.json_text(text, relpath)
            data = new_text.encode("utf-8")
            if data == raw:
                san.verbatim += 1
            # Self-verification.
            before, after = parse(text), parse(new_text)
            if key_sequence(before) != key_sequence(after):
                failures.append("%s: key sequence changed" % relpath)
            leaks = census(new_text, relpath)
            if leaks:
                failures.append("%s: residual %s" % (relpath, ",".join(leaks)))
            embedded = set()
            embedded_data_strings(before, embedded)
            for path, was, _ in differing_strings(before, after):
                if not rule_explains(path, was, embedded):
                    failures.append("%s: value changed without a rule" % relpath)
        else:
            with open(source, "rb") as f:
                data = f.read()
            san.verbatim += 1
        with open(target, "wb") as f:
            f.write(data)

    if failures:
        for line in failures:
            sys.stderr.write(line + "\n")
        raise SystemExit(1)

    print("files total:            %d" % len(files))
    print("S1 files touched:       %d" % san.s1_files)
    print("S2 distinct GUIDs:      %d" % len(san.guid_map))
    print("S3 IPv4 occurrences:    %d" % san.s3_count)
    print("S4 mail hosts:          %d" % san.s4_count)
    print("S5 jars:                %d" % san.s5_count)
    print("copied verbatim:        %d" % san.verbatim)


if __name__ == "__main__":
    main(sys.argv)

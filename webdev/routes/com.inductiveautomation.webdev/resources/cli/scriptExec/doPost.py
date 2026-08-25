def doPost(request, session):
	# ign-cli WebDev route: scriptExec -- SHARED-SECRET-GATED script execution.
	#
	# SECURITY POSTURE (planner-LOCKED with the user's decision: own auth, never
	# wide-open). Anyone with HTTP reach to the gateway must not be able to
	# invoke arbitrary script execution by mere route presence:
	#   - The in-code shared-secret gate below is THE auth mechanism. It is
	#     ported from WHK-Global's production webdev_auth module: dual-header
	#     extraction (case-insensitive), sha256-both-sides constant-time compare
	#     (Jython 2.7 lacks hmac.compare_digest), fail-closed on unconfigured.
	#   - config.json is deliberately require-auth FALSE / user-source "": API
	#     tokens do NOT authenticate WebDev require-auth routes (live-proven
	#     401), so a Basic layer would lock the CLI's own token-authed calls
	#     out, and user-source "default" breaks on renamed IdPs (research Open
	#     Question 3, resolved to secret-only).
	#   - Denials ride the body envelope at HTTP 200 (WebDev ignores 'status').
	#
	# TEMPLATE CONTRACT (read before editing): `ign webdev deploy` (05-03)
	# generates the deployed copy of this file from this template with ONE
	# string substitution -- the marker token on the SECRET line below is
	# replaced by a generated hex secret. As shipped here, SECRET therefore
	# holds the un-substituted placeholder; the gate treats BOTH the None
	# default AND any placeholder-shaped value (leading underscore) as
	# UNCONFIGURED and rejects EVERY action, version included (fail-closed --
	# the WHK _UNCONFIGURED precedent). A deployed secret is hex and can never
	# start with an underscore. This template is deliberately NOT part of
	# ignition-core's ROUTE_FILES bundle, so it cannot be deployed
	# unsubstituted by accident.
	#
	# Action-dispatch contract (doPost only, JSON body): {"action": "<name>", ...}
	#   version -- handshake: {routeVersion, minCli}  (ALSO secret-gated)
	#   exec    -- {code} -> {stdout, result, elapsedMs}. Single-expression code
	#              is eval'd and its value returned as result; statement code is
	#              exec'd and an optional `_result` global is surfaced. stdout is
	#              captured and restored. Every invocation is audit-logged
	#              (sha256-prefix + elapsedMs) via system.util.logger.
	#
	# Body envelope (WebDev IGNORES the 'status' key -- denials ride HTTP 200):
	#   success: {"ok": true, "data": {...}}
	#   failure: {"ok": false, "error": {"code": "secret_required"|"secret_mismatch"|...,
	#                                    "message": "<human>",
	#                                    "traceback": <optional>}}
	#
	# SELF-CONTAINED BY DESIGN: WebDev route folders are independent (no
	# cross-resource imports), so the shared core (unicode re-parse, jv()
	# walker, envelope) is duplicated across the five cli/* routes
	# deliberately. Do not "fix" the duplication by importing.

	ROUTE_VERSION = '1.0.0'  # same constants in every route + ROUTE_BUNDLE_VERSION in ignition-core
	MIN_CLI = '1.0'

	# Deploy-time substitution target: the marker inside the string below is
	# replaced with the generated hex secret; until then this is the placeholder.
	SECRET = None or '__IGN_CLI_SECRET__'


	def _secretUnconfigured():
		# None default OR placeholder-shaped (leading underscore -- a deployed
		# secret is hex and can never start with one) = fail closed.
		return SECRET is None or str(SECRET).startswith('_')


	def _sha256Hex(s):
		from java.security import MessageDigest
		md = MessageDigest.getInstance('SHA-256')
		md.update(str(s).encode('utf-8'))
		digest = md.digest()
		return ''.join(['%02x' % (b & 0xFF) for b in digest])


	def _constantTimeEquals(a, b):
		# Hash both sides first: the digests are always 64 hex chars, so the
		# comparison loop is fixed-length regardless of input (no length leak).
		da = _sha256Hex(a)
		db = _sha256Hex(b)
		if len(da) != len(db):
			return False
		result = 0
		for i in range(len(da)):
			result |= ord(da[i]) ^ ord(db[i])
		return result == 0


	def _extractSecret(request):
		# Dual-header extract, case-insensitive: WebDev hands back headers with
		# whatever casing the client sent, so lower-case the dict before lookup.
		headers = request.get('headers', {}) or {}
		lowered = {}
		for key, value in headers.items():
			try:
				lowered[key.lower()] = value
			except AttributeError:
				# Non-string header key -- ignore rather than blow up the handler.
				continue
		presented = lowered.get('x-ignition-cli-secret', '') or ''
		if presented:
			return presented.strip()
		auth = lowered.get('authorization', '') or ''
		if auth.startswith('Bearer '):
			return auth[7:].strip()
		return ''

	import json, traceback, time, sys
	from StringIO import StringIO
	data = request['data']
	if isinstance(data, (str, unicode)):  # Pitfall 3: parsed dict for JSON bodies, str/unicode only when malformed
		data = json.loads(data)
	action = data.get('action')

	def ok(payload):
		return {'json': {'ok': True, 'data': payload}}

	def err(code, message, tb=None):
		e = {'code': code, 'message': message}
		if tb:
			e['traceback'] = tb
		return {'json': {'ok': False, 'error': e}}

	def jv(x, depth=0):
		# jsonEncode stack-overflows on Java objects; walk manually.
		if depth > 12:
			return str(x)
		if x is None or isinstance(x, (bool, int, long, float)):
			return x
		if isinstance(x, (str, unicode)):
			return str(x)
		if isinstance(x, (list, tuple)):
			return [jv(i, depth + 1) for i in x]
		if isinstance(x, dict):
			out = {}
			for k in x.keys():
				out[str(k)] = jv(x.get(k), depth + 1)
			return out
		try:
			if hasattr(x, 'keySet'):
				out = {}
				for k in x.keySet():
					out[str(k)] = jv(x.get(k), depth + 1)
				return out
		except:
			pass
		return str(x)

	# FAIL-CLOSED GATE -- before ANY action dispatch, version included.
	if _secretUnconfigured():
		return err('secret_required', 'scriptExec route secret is not configured -- (re)deploy this route via ign webdev deploy')
	presented = _extractSecret(request)
	if not presented:
		return err('secret_required', 'missing x-ignition-cli-secret (or Authorization: Bearer) header')
	if not _constantTimeEquals(presented, str(SECRET)):
		return err('secret_mismatch', 'scriptExec secret mismatch')

	log = system.util.logger('ign-cli-scriptexec')

	try:
		if action == 'version':
			return ok({'routeVersion': ROUTE_VERSION, 'minCli': MIN_CLI})

		if action == 'exec':
			code = data['code']
			started = time.time()
			g = {}
			captured = StringIO()
			oldStdout = sys.stdout
			result = None
			try:
				sys.stdout = captured
				try:
					result = eval(code, g)  # single-expression code returns its value
				except SyntaxError:
					exec code in g  # statement code (STATEMENT form: the exec(...) call form trips this Jython build at depth — live-bisected 05-06)
					result = g.get('_result')
			finally:
				sys.stdout = oldStdout
				# Audit pattern: log code-hash prefix + elapsed for EVERY invocation.
				log.info('exec sha256=%s elapsedMs=%d' % (_sha256Hex(code)[:12], int((time.time() - started) * 1000)))
			return ok({
				'stdout': str(captured.getvalue()),
				'result': jv(result),
				'elapsedMs': int((time.time() - started) * 1000),
			})

		return err('unknown_action', 'unknown action: ' + str(action))
	except:
		# Bare except -- catches Java Throwables too, keeping the body JSON (not a Jetty HTML 500).
		return err('route_error', 'scriptExec route error', traceback.format_exc())

def doPost(request, session):
	# ign-cli WebDev route: tagDelete -- the authenticated, scope-enforcing tag
	# delete primitive (Phase 26, D-13, 26-17).
	#
	# SECURITY POSTURE. Three layers must all pass before a tag is deleted:
	#   1. WebDev itself: config.json is require-auth TRUE with the install's
	#      named user source and its deploy role in required-roles. WebDev
	#      answers 401 without a Basic user of that source (an API token does
	#      not authenticate a require-auth route) and refuses a user without
	#      the role before this code runs.
	#   2. The deploy-time secret below, read ONLY from the
	#      x-ignition-cli-secret header (never from Authorization, which
	#      carries Basic) and compared sha256-both-sides in constant time
	#      (Jython 2.7 lacks hmac.compare_digest). Fail closed: an
	#      unsubstituted or missing secret refuses every action, version
	#      included.
	#   3. Scope: one provider per call, an exact member of the install-time
	#      allowlist; at most MAX_PATHS paths, each a non-root path under that
	#      provider. Every path is validated before any delete.
	#
	# TEMPLATE CONTRACT (read before editing): `ign webdev deploy
	# --with-tag-delete` substitutes the two markers below (the secret and the
	# JSON-encoded provider list) and the role and user source in config.json.
	# Until then the route is UNCONFIGURED: SECRET or PROVIDERS_JSON None,
	# placeholder-shaped (leading underscore) or an empty list refuses every
	# action. A deployed secret is hex and never starts with an underscore.
	# This template is NOT part of ignition-core's ROUTE_FILES bundle, so it
	# cannot ship unsubstituted.
	#
	# Action-dispatch contract (doPost only, JSON body): {"action": "<name>", ...}
	#   version -- {routeVersion, minCli, providers, maxPaths, maxPathChars}
	#   delete  -- {provider, paths} (paths relative to the provider, no
	#              brackets) -> {provider, results: [{path, quality, good,
	#              existsAfter}], elapsedMs}, one result per path in request
	#              order. quality/good are null when deleteTags returns no
	#              per-path list; existsAfter is null when the exists read
	#              after the call raised (not confirmed either way).
	#
	# AUDIT. Logger ign-cli-tagdelete gets a delete-begin line before the
	# deleteTags call and a delete-end line in a finally after it, so a call
	# that raises still leaves a record of who asked for what.
	#
	# Body envelope (WebDev IGNORES the 'status' key -- denials ride HTTP 200):
	#   success: {"ok": true, "data": {...}}
	#   failure: {"ok": false, "error": {"code": "secret_required"|"secret_mismatch"|
	#             "unknown_action"|"provider_not_allowed"|"path_invalid"|
	#             "too_many_paths"|"route_error", "message": "<human>",
	#             "traceback": <optional>}}
	#
	# The only tag write this route makes is the one deleteTags call. SELF-CONTAINED
	# BY DESIGN: no cross-resource imports; the helpers are copied from scriptExec.

	ROUTE_VERSION = '1.3.0'  # same constants in every route + ROUTE_BUNDLE_VERSION in ignition-core
	MIN_CLI = '1.0'
	MAX_PATHS = 500
	MAX_PATH_CHARS = 1024

	# Deploy-time substitution targets (placeholders until installed).
	SECRET = None or '__IGN_TAG_DELETE_SECRET__'
	PROVIDERS_JSON = None or '__IGN_TAG_DELETE_PROVIDERS__'

	import json, traceback, time

	def ok(payload):
		return {'json': {'ok': True, 'data': payload}}

	def err(code, message, tb=None):
		e = {'code': code, 'message': message}
		if tb:
			e['traceback'] = tb
		return {'json': {'ok': False, 'error': e}}

	def _placeholder(value):
		return value is None or str(value).startswith('_')

	def _providers():
		# None when unconfigured; the parsed list of names otherwise.
		if _placeholder(PROVIDERS_JSON):
			return None
		try:
			parsed = json.loads(PROVIDERS_JSON)
		except:
			return None
		if not isinstance(parsed, list) or len(parsed) == 0:
			return None
		for p in parsed:
			if not isinstance(p, (str, unicode)):
				return None
		return list(parsed)

	def _sha256Hex(s):
		from java.security import MessageDigest
		md = MessageDigest.getInstance('SHA-256')
		if isinstance(s, unicode):
			md.update(s.encode('utf-8'))
		else:
			md.update(str(s))
		digest = md.digest()
		return ''.join(['%02x' % (b & 0xFF) for b in digest])

	def _constantTimeEquals(a, b):
		# Hash both sides first: fixed-length comparison, no length leak.
		da = _sha256Hex(a)
		db = _sha256Hex(b)
		if len(da) != len(db):
			return False
		result = 0
		for i in range(len(da)):
			result |= ord(da[i]) ^ ord(db[i])
		return result == 0

	def _extractSecret(request):
		# The x-ignition-cli-secret header only, case-insensitive.
		headers = request.get('headers', {}) or {}
		lowered = {}
		for key, value in headers.items():
			try:
				lowered[key.lower()] = value
			except AttributeError:
				continue
		presented = lowered.get('x-ignition-cli-secret', '') or ''
		return presented.strip()

	def _userName(session):
		# The session user's name for the audit line; never raises.
		try:
			user = session['user']
		except:
			return None
		if user is None:
			return None
		for getter in ('getUsername', 'getName'):
			try:
				return unicode(getattr(user, getter)())
			except:
				pass
		try:
			return unicode(user.get('username'))
		except:
			return None

	def _audit(line):
		# One audit line; a logging failure must not hide a delete.
		try:
			system.util.logger('ign-cli-tagdelete').info(line)
		except:
			pass

	def _pathProblem(p):
		# None when p is a valid non-root path under the provider.
		if not isinstance(p, (str, unicode)):
			return 'is not a string'
		if len(p) < 1 or len(p) > MAX_PATH_CHARS:
			return 'must be 1 to %d characters' % MAX_PATH_CHARS
		for ch in p:
			if ord(ch) < 32 or ord(ch) == 127:
				return 'holds a control character'
		if '[' in p or ']' in p:
			return 'holds a bracket (paths are relative to the provider)'
		if p.startswith('/') or p.endswith('/'):
			return 'has a leading or trailing slash'
		for segment in p.split('/'):
			if segment in ('', '.', '..'):
				return 'has an empty, . or .. segment'
		if p.lower() == '_types_':
			return 'names the _types_ folder itself'
		return None

	# FAIL-CLOSED GATE -- before ANY action dispatch, version included.
	providers = _providers()
	if _placeholder(SECRET) or providers is None:
		return err('secret_required', 'tagDelete route is not configured -- (re)deploy it with ign webdev deploy --with-tag-delete')
	presented = _extractSecret(request)
	if not presented:
		return err('secret_required', 'missing x-ignition-cli-secret header')
	if not _constantTimeEquals(presented, str(SECRET)):
		return err('secret_mismatch', 'tagDelete secret mismatch')

	try:
		data = request['data']
		if isinstance(data, (str, unicode)):
			data = json.loads(data)
		if not hasattr(data, 'get'):
			return err('unknown_action', 'the body must be a JSON object with an action')
		action = data.get('action')

		if action == 'version':
			return ok({
				'routeVersion': ROUTE_VERSION,
				'minCli': MIN_CLI,
				'providers': providers,
				'maxPaths': MAX_PATHS,
				'maxPathChars': MAX_PATH_CHARS,
			})

		if action == 'delete':
			provider = data.get('provider')
			if not isinstance(provider, (str, unicode)) or provider not in providers:
				return err('provider_not_allowed', 'provider is not in this route\'s allowlist')
			paths = data.get('paths')
			if not isinstance(paths, list) or len(paths) == 0 or len(paths) > MAX_PATHS:
				return err('too_many_paths', 'paths must be a list of 1 to %d paths' % MAX_PATHS)
			for index in range(len(paths)):
				problem = _pathProblem(paths[index])
				if problem is not None:
					return err('path_invalid', 'paths[%d] %s' % (index, problem))

			started = time.time()
			full = [u'[%s]%s' % (provider, p) for p in paths]
			try:
				digest = _sha256Hex(u'\n'.join(paths))[:12]
			except:
				digest = u'unknown'
			user = _userName(session)
			_audit(u'delete-begin user=%s provider=%s count=%d paths_sha256=%s' % (
				user, provider, len(paths), digest))
			outcome = u'raised'
			try:
				returned = system.tag.deleteTags(full)
				outcome = u'returned'
			finally:
				_audit(u'delete-end user=%s provider=%s count=%d paths_sha256=%s outcome=%s elapsedMs=%d' % (
					user, provider, len(paths), digest, outcome,
					int((time.time() - started) * 1000)))
			codes = None
			try:
				if returned is not None and len(returned) == len(full):
					codes = list(returned)
			except:
				codes = None
			results = []
			for index in range(len(paths)):
				quality = None
				good = None
				if codes is not None:
					code = codes[index]
					quality = str(code) if code is not None else None
					try:
						good = bool(code.isGood())
					except:
						good = None
				# A read that raises must not hide a delete that happened:
				# null tells the client the path is not confirmed either way.
				try:
					existsAfter = bool(system.tag.exists(full[index]))
				except:
					existsAfter = None
				results.append({
					'path': paths[index],
					'quality': quality,
					'good': good,
					'existsAfter': existsAfter,
				})
			elapsed = int((time.time() - started) * 1000)
			return ok({'provider': provider, 'results': results, 'elapsedMs': elapsed})

		return err('unknown_action', 'unknown action: ' + str(action))
	except:
		# Bare except -- catches Java Throwables too, keeping the body JSON.
		return err('route_error', 'tagDelete route error', traceback.format_exc())

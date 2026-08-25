def doPost(request, session):
	# ign-cli WebDev route: tags -- live tag VALUE operations.
	#
	# Action-dispatch contract (doPost only, JSON body): {"action": "<name>", ...}
	#   version -- handshake: {routeVersion, minCli}
	#   browse  -- {path=''} -> tag-tree entries; discriminator is tagType
	#              (Provider/Folder/AtomicTag/UdtType/UdtInstance/Property). ALL
	#              entries pass through, Property children INCLUDED -- filtering
	#              is the CLI's display decision.
	#   read    -- {paths: [...]} -> per-path {path, value, quality, timestamp}
	#   write   -- {path, value} -> {path, quality}
	#
	# Body envelope (WebDev IGNORES the 'status' key -- denials ride HTTP 200):
	#   success: {"ok": true, "data": {...}}
	#   failure: {"ok": false, "error": {"code": "<machine_code>",
	#                                    "message": "<human>",
	#                                    "traceback": <optional>}}
	#
	# SELF-CONTAINED BY DESIGN: WebDev route folders are independent (no
	# cross-resource imports), so the ~25-line shared core (unicode re-parse,
	# jv() walker, envelope) is duplicated across the five cli/* routes
	# deliberately. Do not "fix" the duplication by importing.
	#
	# Every scripting call below is the LIVE-PROVEN form from the Phase 5
	# research probe (05-RESEARCH.md). Do not "modernize" them.

	ROUTE_VERSION = '1.0.0'  # same constants in every route + ROUTE_BUNDLE_VERSION in ignition-core
	MIN_CLI = '1.0'

	import json, traceback
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
		# jsonEncode stack-overflows on Java objects (TagPath, enums); walk manually.
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

	def entry(r):
		# Browse results are dicts (research-verified); attribute fallback for safety.
		try:
			return {
				'fullPath': str(r['fullPath']),
				'name': str(r['name']),
				'tagType': str(r.get('tagType')),
				'hasChildren': bool(r.get('hasChildren')),
				'dataType': str(r['dataType']) if r.get('dataType') else None,
			}
		except:
			return {
				'fullPath': str(r.fullPath),
				'name': str(r.name),
				'tagType': str(getattr(r, 'tagType', '')),
				'hasChildren': False,
				'dataType': None,
			}

	try:
		if action == 'version':
			return ok({'routeVersion': ROUTE_VERSION, 'minCli': MIN_CLI})

		if action == 'browse':
			path = data.get('path', '')
			res = system.tag.browse(path, {})
			out = []
			for r in res.getResults():
				out.append(entry(r))
			return ok({'results': out})

		if action == 'read':
			paths = data['paths']
			qvs = system.tag.readBlocking(paths)
			out = []
			for i, p in enumerate(paths):
				qv = qvs[i]
				out.append({
					'path': p,
					'value': jv(qv.value),
					'quality': str(qv.quality),
					'timestamp': str(qv.timestamp),
				})
			return ok({'results': out})

		if action == 'write':
			p = data['path']
			v = data['value']
			qv = system.tag.writeBlocking([p], [v])
			# Quality has no .quality attr -- str() the element (prior-art defect).
			return ok({'results': [{'path': p, 'quality': str(qv[0])}]})

		return err('unknown_action', 'unknown action: ' + str(action))
	except:
		# Bare except -- catches Java Throwables too, keeping the body JSON (not a Jetty HTML 500).
		return err('route_error', 'tags route error', traceback.format_exc())

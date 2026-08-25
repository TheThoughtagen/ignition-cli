def doPost(request, session):
	# ign-cli WebDev route: tagConfig -- tag CONFIGURATION CRUD, UDTs, bulk export.
	#
	# Action-dispatch contract (doPost only, JSON body): {"action": "<name>", ...}
	#   version          -- handshake: {routeVersion, minCli}
	#   getConfig        -- {tagPath, recursive=False} -> {config}
	#   configure        -- {basePath, tags, collisionPolicy='m'} -> {results}
	#   deleteTags       -- {paths: [...]} -> {deleted}
	#   listUDTTypes     -- {provider='default'} -> {results} (browse entry shape)
	#   getUDTDefinition -- {provider, name} -> {definition}
	#   exportTags       -- {paths: [...]} -> {payload: <json string>}
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
	# Every ignition-mcp prior-art defect is CORRECTED here (live-verified forms
	# from 05-RESEARCH.md -- do not regress them):
	#   - getConfiguration takes a STRING first arg; a list's '[' poisons
	#     TagPathParser with a misleading TagPathFormatException.
	#   - configure takes a basePath ('[default]'), NEVER a provider name.
	#   - children NEST under Folder/UdtType entries; slash-names are rejected.
	#   - alarms are a LIST of dicts; a name-keyed dict is silently ignored.
	#   - tagType (not 'type') is the discriminator.
	#   - exportTags is KWARGS-ONLY (tagPaths=...); the positional form fails.

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

		if action == 'getConfig':
			tagPath = data['tagPath']  # STRING -- the list form poisons TagPathParser
			recursive = bool(data.get('recursive', False))
			tags = system.tag.getConfiguration(tagPath, recursive)
			if tags and tags[0]:
				return ok({'config': jv(tags[0])})
			return err('not_found', 'not found: ' + str(tagPath))

		if action == 'configure':
			base = data.get('basePath', '[default]')  # basePath form, NOT a provider name
			tags = data['tags']
			collisionPolicy = data.get('collisionPolicy', 'm')
			result = system.tag.configure(base, tags, collisionPolicy)
			# Quality strings verbatim: Good / Bad_NotFound(...) / Error_Configuration(...) / Bad_Unsupported(...)
			return ok({'results': [str(x) for x in result]})

		if action == 'deleteTags':
			paths = data['paths']
			system.tag.deleteTags(paths)  # returns nothing; count echoes the request length
			return ok({'deleted': len(paths)})

		if action == 'listUDTTypes':
			prov = data.get('provider', 'default')
			res = system.tag.browse('[%s]_types_' % prov, {})
			out = []
			for r in res.getResults():
				out.append(entry(r))
			return ok({'results': out})

		if action == 'getUDTDefinition':
			prov = data['provider']
			name = data['name']
			tags = system.tag.getConfiguration('[%s]_types_/%s' % (prov, name), True)
			if tags and tags[0]:
				return ok({'definition': jv(tags[0])})
			return err('not_found', 'udt not found: [%s]_types_/%s' % (prov, name))

		if action == 'exportTags':
			payload = system.tag.exportTags(tagPaths=data['paths'])  # kwargs ONLY (positional fails)
			return ok({'payload': str(payload)})

		return err('unknown_action', 'unknown action: ' + str(action))
	except:
		# Bare except -- catches Java Throwables too, keeping the body JSON (not a Jetty HTML 500).
		return err('route_error', 'tagConfig route error', traceback.format_exc())

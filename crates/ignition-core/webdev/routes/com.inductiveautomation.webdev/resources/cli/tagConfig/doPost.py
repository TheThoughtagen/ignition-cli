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
	#   exportTags       -- {paths: [...], format='json'} -> {payload: <json
	#                       string>} -- or, format='xml': {payload_b64, format}
	#                       (base64 of the gateway's CRLF XML document; byte-
	#                       exact through the JSON envelope)
	#   importTagsFile   -- {file_b64, basePath='[default]', collisionPolicy='a',
	#                       format='json'|'xml'|'csv'}
	#                       -> {results: [<QualityCode strings>]} -- base64 file
	#                       bytes to a gateway-side temp file (suffix per format:
	#                       importTags dispatches its parser on the file
	#                       EXTENSION, 11-06 live truth), system.tag.
	#                       importTags (FILE-PATH-only signature, 11-01
	#                       Probe 2), temp deleted in a finally
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
	#   - provider-ROOT paths ('[default]' alone / a bare provider name) refuse
	#     'provider_root_unsupported' (getConfiguration/exportTags need an
	#     RpcContext WebDev threads don't carry, 8.3.3) -- subtree paths like
	#     [provider]folder are the supported form.

	ROUTE_VERSION = '1.3.0'  # 1.3.0: importTagsFile format-aware temp suffix (11-06 live-truth fix; 1.2.0: exportTags format param + importTagsFile)
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

	def is_provider_root(p):
		# Bracket-form provider ROOT only (07-06): '[default]' / '[prov]' --
		# the remainder after ']' empty or just slashes. system.tag
		# getConfiguration/exportTags need an RpcContext WebDev threads do
		# not carry for a provider root (8.3.3 b2026012009) -- refuse
		# honestly instead of surfacing the IllegalStateException. The BARE
		# form ('default') cannot be pre-detected without provider-name
		# knowledge; the gateway resolves it to the provider root and the
		# call-site translation below catches the same 'No RpcContext'.
		p = str(p)
		if not p.startswith('['):
			return False
		end = p.find(']')
		if end == -1:
			return False
		return p[end + 1:].strip('/') == ''

	try:
		if action == 'version':
			return ok({'routeVersion': ROUTE_VERSION, 'minCli': MIN_CLI})

		if action == 'getConfig':
			tagPath = data['tagPath']  # STRING -- the list form poisons TagPathParser
			recursive = bool(data.get('recursive', False))
			# Provider-root pre-call refusal (07-06): zero gateway work, deterministic.
			if is_provider_root(tagPath):
				return err('provider_root_unsupported', 'provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder')
			try:
				tags = system.tag.getConfiguration(tagPath, recursive)
			except:
				# RpcContext translation (07-06): a BARE provider-matching
				# first segment resolved to the provider root inside
				# TagPathParser and threw -- the same honest refusal. Every
				# other exception re-raises into the outer bare-exect
				# (generic route_error semantics unchanged).
				if 'No RpcContext' in traceback.format_exc():
					return err('provider_root_unsupported', 'provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder')
				raise
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
			paths = data['paths']
			fmt = data.get('format', 'json')  # the CLI flag's vocabulary; maps to the gateway's exportType kwarg
			# Provider-root pre-flight scan (07-06): refuse naming the
			# offending path BEFORE any gateway work.
			for p in paths:
				if is_provider_root(p):
					return err('provider_root_unsupported', 'provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder: ' + str(p))
			if fmt == 'json':
				# The contract-frozen JSON interchange -- byte-identical to
				# the 1.1.0 behavior (existing pins keep passing untouched).
				try:
					payload = system.tag.exportTags(tagPaths=paths)  # kwargs ONLY (positional fails)
				except:
					# RpcContext translation (07-06): the bare provider form.
					if 'No RpcContext' in traceback.format_exc():
						return err('provider_root_unsupported', 'provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder')
					raise
				return ok({'payload': str(payload)})
			if fmt == 'xml':
				# XML out: the gateway's full CRLF document rides base64 so
				# it survives the JSON envelope byte-exactly (research
				# Pattern 1 -- no string-escaping edge cases). Primary path
				# is the kwargs+xml form live-proven by 11-01 Probe 1 (both
				# rigs: full CRLF XML document string, no declaration).
				import base64
				try:
					payload = system.tag.exportTags(tagPaths=paths, exportType='xml')
				except:
					if 'No RpcContext' in traceback.format_exc():
						return err('provider_root_unsupported', 'provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder')
					# TEMP-FILE FALLBACK (research Pitfall 1): the kwargs+xml
					# form may differ from the docs' file-path signature on
					# some builds -- fall back to the documented positional
					# form (arg-1 is the filePath the export is WRITTEN to,
					# 11-01 Probe 1c), read the file, delete it.
					from java.io import File as _F
					_tmp = _F.createTempFile('ign-export', '.xml')
					try:
						system.tag.exportTags(_tmp.getAbsolutePath(), paths, True, 'xml')
						_fh = open(_tmp.getAbsolutePath(), 'rb')
						_bytes = _fh.read()
						_fh.close()
						payload = _bytes  # bytes path below handles str-vs-bytes
					finally:
						_tmp.delete()
				# Normalize to a byte string for the b64 carrier: the
				# kwargs+xml form returns unicode (Jython str = byte str).
				if isinstance(payload, unicode):
					payload_bytes = payload.encode('utf-8')
				else:
					payload_bytes = str(payload)
				return ok({'payload_b64': base64.b64encode(payload_bytes), 'format': 'xml'})
			return err('unsupported_format', 'supported formats: json, xml')

		if action == 'importTagsFile':
			# File-path import (Phase 11): base64 file bytes in ->
			# gateway-side temp file -> system.tag.importTags (the
			# FILE-PATH-only signature, 11-01 Probe 2) -> temp deleted.
			# 11-06 LIVE-TRUTH FIX: importTags dispatches its parser on
			# the file EXTENSION (.xml -> XML, .csv -> CSV, anything
			# else -> JSON) -- the 1.2.0 temp file ('.tagimport') made
			# XML/CSV imports hit the JSON parser
			# (Error_Exception("Not a JSON Object: ..."), caught live by
			# the 11-06 fidelity gate on both rigs; Probe 2's probe file
			# was '.xml'-suffixed, which is why the probe passed where
			# the route failed). The body therefore carries an explicit
			# `format` ('xml'|'csv') and the temp file takes the
			# matching suffix; json keeps '.tagimport' (the JSON parser
			# is the any-extension default).
			file_b64 = data['file_b64']
			base = data.get('basePath', '[default]')
			policy = data.get('collisionPolicy', 'a')
			fmt = data.get('format', 'json')
			suffix = {'xml': '.xml', 'csv': '.csv'}.get(fmt, '.tagimport')
			if fmt not in ('xml', 'csv', 'json'):
				return err('unsupported_format', "supported formats: json, xml, csv")
			# LOCKED collision matrix (05-05): abort/overwrite only.
			# The gateway's 'i' (Ignore) exists but is NEVER surfaced.
			if policy not in ('a', 'o'):
				return err('invalid_collision_policy', "collisionPolicy must be 'a' (abort) or 'o' (overwrite)")
			import base64
			from java.io import File
			tmp = File.createTempFile('ign-import', suffix)
			try:
				fh = open(tmp.getAbsolutePath(), 'wb')
				fh.write(base64.b64decode(file_b64))
				fh.close()
				try:
					results = system.tag.importTags(tmp.getAbsolutePath(), base, policy)
				except:
					# 07-06 translation, kept defensively: 11-01 Probe 2
					# proved importTags FREE of the RpcContext constraint
					# from a SCRIPT thread (provider-root basePath works);
					# WebDev-thread truth is the 11-06 live gate's job, so
					# a No-RpcContext throw here still refuses honestly.
					if 'No RpcContext' in traceback.format_exc():
						return err('provider_root_unsupported', 'provider-root tag paths are not supported on WebDev threads (no RpcContext) -- use a subtree path like [provider]folder')
					raise
			finally:
				tmp.delete()  # cleanup on EVERY path -- leaked temp files on a production gateway are a real failure mode
			return ok({'results': [str(x) for x in results]})

		return err('unknown_action', 'unknown action: ' + str(action))
	except:
		# Bare except -- catches Java Throwables too, keeping the body JSON (not a Jetty HTML 500).
		return err('route_error', 'tagConfig route error', traceback.format_exc())

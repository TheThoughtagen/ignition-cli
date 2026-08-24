# ign-cli WebDev route: tagHistory -- historical tag value queries.
#
# Action-dispatch contract (doPost only, JSON body): {"action": "<name>", ...}
#   version -- handshake: {routeVersion, minCli}
#   query   -- {paths: [...], startDateMs, endDateMs,
#               aggregationMode='LastValue', returnSize=N} -> {columns, rows,
#               rowCount}. The time column is 't_stamp' (NOT 'Timestamp' --
#               prior-art defect 8) and is passed through VERBATIM; tag
#               columns are provider-relative paths. Every cell rides jv().
#
#   The query PATH is structurally safe on default rigs (zero historians):
#   it returns a well-formed dataset with null values. Data requires a
#   historian (InternalHistorian is provisionable via native REST).
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


def doPost(request, session):
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

	try:
		if action == 'version':
			return ok({'routeVersion': ROUTE_VERSION, 'minCli': MIN_CLI})

		if action == 'query':
			from java.util import Date
			paths = data['paths']
			kw = {'paths': paths}
			aggregation = data.get('aggregationMode', 'LastValue')
			if aggregation:
				kw['aggregationMode'] = aggregation
			if data.get('returnSize') is not None:
				kw['returnSize'] = data['returnSize']
			# long() wrap is MANDATORY: Date(float) fails Java coercion (Pitfall 12).
			kw['startDate'] = Date(long(data['startDateMs']))
			kw['endDate'] = Date(long(data['endDateMs']))
			ds = system.tag.queryTagHistory(**kw)
			columns = []
			for c in range(ds.columnCount):
				# 't_stamp' and provider-relative tag paths pass through VERBATIM.
				columns.append(str(ds.getColumnName(c)))
			rows = []
			for r in range(ds.rowCount):
				rows.append([jv(ds.getValueAt(r, c)) for c in range(ds.columnCount)])
			return ok({'columns': columns, 'rows': rows, 'rowCount': ds.rowCount})

		return err('unknown_action', 'unknown action: ' + str(action))
	except:
		# Bare except -- catches Java Throwables too, keeping the body JSON (not a Jetty HTML 500).
		return err('route_error', 'tagHistory route error', traceback.format_exc())

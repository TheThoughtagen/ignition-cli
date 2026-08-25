def doPost(request, session):
	# ign-cli WebDev route: alarms -- active alarm status, journal history, acknowledge.
	#
	# Action-dispatch contract (doPost only, JSON body): {"action": "<name>", ...}
	#   version      -- handshake: {routeVersion, minCli}
	#   active       -- {source?, priority?, state?} -> {results, count}; entries
	#                   carry {eventId, source, state, priority, name} (eventId is
	#                   a UUID object -- stringified; state strings look like
	#                   'Active, Unacknowledged').
	#   history      -- {startDateMs?, endDateMs?} -> {results, count}; on a rig
	#                   with no alarm journal profile configured, returns the
	#                   STRUCTURED denial code no_alarm_journal (the CLI maps it
	#                   to an actionable slug) -- default rigs always hit this.
	#   acknowledge  -- {eventIds: [...], note='', username='ign-cli'} ->
	#                   {unacknowledged: [...]}; the gateway-scope 3-arg form
	#                   (String[] eventIds, note, username) -- the 2-arg form
	#                   fails, and UUID objects don't coerce to String[].
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
	import java.lang.Throwable
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

		if action == 'active':
			kw = {}
			if data.get('source') is not None:
				kw['source'] = data['source']
			if data.get('priority') is not None:
				kw['priority'] = data['priority']
			if data.get('state') is not None:
				kw['state'] = data['state']
			alarms = system.alarm.queryStatus(**kw)
			out = []
			for a in alarms:
				out.append({
					'eventId': str(a.eventId),  # UUID object -- stringify
					'source': str(a.source),
					'state': str(a.state),
					'priority': str(a.priority),
					'name': str(a.name) if hasattr(a, 'name') else None,
				})
			return ok({'results': out, 'count': len(out)})

		if action == 'history':
			from java.util import Date
			kw = {}
			if data.get('startDateMs') is not None:
				kw['startDate'] = Date(long(data['startDateMs']))  # long() wrap: Date(float) fails coercion (Pitfall 12)
			else:
				kw['startDate'] = None
			if data.get('endDateMs') is not None:
				kw['endDate'] = Date(long(data['endDateMs']))
			else:
				kw['endDate'] = None
			try:
				entries = system.alarm.queryJournal(**kw)
			except java.lang.Throwable, jt:
				# Default rigs have no alarm journal profile (journal = database
				# connection + alarm-journal profile chain). Return the STRUCTURED
				# denial so the CLI can map it to an actionable slug.
				if 'No alarm journal profile specified' in str(jt):
					return err('no_alarm_journal', str(jt))
				raise
			except Exception, e:
				if 'No alarm journal profile specified' in str(e):
					return err('no_alarm_journal', str(e))
				raise
			out = []
			for e in entries:
				row = {}
				for f in ('eventId', 'source', 'state', 'priority', 'name', 'eventData'):
					try:
						v = getattr(e, f)
						row[f] = str(v) if v is not None else None
					except:
						row[f] = None
				out.append(row)
			return ok({'results': out, 'count': len(out)})

		if action == 'acknowledge':
			ids = [str(i) for i in data['eventIds']]  # UUID objects don't coerce to String[]
			note = data.get('note', '')
			username = data.get('username', 'ign-cli')
			# Gateway-scope 3-arg form: (String[] eventIds, note, username). The 2-arg form fails.
			failed = system.alarm.acknowledge(ids, note, username)
			# 8.3 behavior: returns the list of UNacknowledged ids.
			return ok({'unacknowledged': [str(x) for x in (failed or [])]})

		return err('unknown_action', 'unknown action: ' + str(action))
	except:
		# Bare except -- catches Java Throwables too, keeping the body JSON (not a Jetty HTML 500).
		return err('route_error', 'alarms route error', traceback.format_exc())

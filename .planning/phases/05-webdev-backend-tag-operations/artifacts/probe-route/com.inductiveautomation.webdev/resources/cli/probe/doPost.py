def doPost(request, session):
	import json, traceback, time
	import java.lang.Throwable
	data = request['data']
	if isinstance(data, (str, unicode)):
		data = json.loads(data)
	action = data.get('action')
	def ok(payload):
		return {'json': payload}
	def err(code, msg, tb=None):
		return {'json': {'error': msg, 'traceback': tb}}
	def jv(x, depth=0):
		if depth > 12:
			return str(x)
		if x is None or isinstance(x, (bool, int, long, float)):
			return x
		if isinstance(x, (str, unicode)):
			return str(x)
		if isinstance(x, (list, tuple)):
			return [jv(i, depth+1) for i in x]
		if isinstance(x, dict):
			out = {}
			for k in x.keys():
				out[str(k)] = jv(x.get(k), depth+1)
			return out
		try:
			if hasattr(x, 'keySet'):
				out = {}
				for k in x.keySet():
					out[str(k)] = jv(x.get(k), depth+1)
				return out
		except:
			pass
		return str(x)
	try:
		if action == 'version':
			return ok({'routeVersion': 'p5-probe-1', 'ignitionVersion': 'see-gateway-info'})

		if action == 'session':
			sk = []
			try:
				sk = sorted([str(k) for k in session.keys()])
			except:
				try:
					sk = sorted([str(k) for k in session.keySet()])
				except:
					sk = ['<unintrospectable>']
			sv = {}
			for k in sk:
				try:
					sv[k] = str(session.get(k))[:120]
				except:
					sv[k] = '<err>'
			return ok({'sessionKeys': sk, 'sessionVals': sv, 'headers': dict(request.get('headers', {})), 'params': dict(request.get('params', {}))})

		if action == 'browse':
			path = data.get('path', '')
			res = system.tag.browse(path, {})
			out = []
			for r in res.getResults():
				try:
					out.append({'fullPath': str(r['fullPath']), 'name': str(r['name']), 'type': str(r.get('tagType', r.get('type'))), 'hasChildren': r.get('hasChildren'), 'dataType': str(r.get('dataType')) if r.get('dataType') else None})
				except:
					out.append({'fullPath': str(r.fullPath), 'name': str(r.name)})
			return ok({'results': out})

		if action == 'read':
			paths = data['paths']
			qvs = system.tag.readBlocking(paths)
			out = []
			for i, p in enumerate(paths):
				qv = qvs[i]
				out.append({'path': p, 'value': qv.value, 'quality': str(qv.quality), 'timestamp': str(qv.timestamp)})
			return ok({'results': out})

		if action == 'write':
			p = data['path']; v = data['value']
			qv = system.tag.writeBlocking([p], [v])
			return ok({'results': [{'path': p, 'quality': str(qv[0])}]})

		if action == 'ackAlarms':
			ids = [str(x) for x in data['eventIds']]
			failed = system.alarm.acknowledge(ids, data.get('note', ''), data.get('username', 'ign-cli'))
			return ok({'acknowledged': len(ids) - len(failed or []), 'failed': [str(x) for x in (failed or [])]})
		if action == 'editAlarmConfig':
			r = system.tag.editAlarmConfig(data['path'], data['alarms'])
			return ok({'result': str(r)})
		if action == 'alarmStates':
			out = {}
			def trial(label, fn):
				try:
					out[label] = {'ok': True, 'r': str(fn())[:400]}
				except java.lang.Throwable, jt:
					out[label] = {'ok': False, 'err': str(jt)[:200]}
				except Exception, e:
					out[label] = {'ok': False, 'err': 'py:' + str(e)[:200]}
			defn = system.tag.getAlarmStates('[default]AlarmTag')
			det = []
			for d in defn:
				det.append({'str': str(d)[:300], 'name': str(d.getName()) if hasattr(d, 'getName') else None})
			out['alarmDefn'] = {'ok': True, 'r': str(det)}
			trial('getAlarmStates', lambda: system.tag.getAlarmStates('[default]AlarmTag'))
			trial('queryStatus_all', lambda: len(list(system.alarm.queryStatus())))
			trial('queryStatus_source', lambda: str(list(system.alarm.queryStatus(source='*AlarmTag*', includeOverview=False)))[:300])
			return ok({'trials': out})
		if action == 'pathProbe':
			out = {}
			def trial(label, fn):
				try:
					out[label] = {'ok': True, 'r': str(fn())[:250]}
				except java.lang.Throwable, jt:
					out[label] = {'ok': False, 'err': str(jt)[:180]}
				except Exception, e:
					out[label] = {'ok': False, 'err': 'py:' + str(e)[:180]}
			trial('exists_bracket', lambda: system.tag.exists('[default]P5/T1'))
			trial('getCfg_str_notlist', lambda: system.tag.getConfiguration('[default]P5/T1', False))
			trial('getCfg_list', lambda: system.tag.getConfiguration(['[default]P5/T1'], False))
			trial('getCfg_folder', lambda: system.tag.getConfiguration(['[default]P5'], False))
			trial('getCfg_provider_only', lambda: system.tag.getConfiguration(['[default]'], False))
			return ok({'trials': out})
		if action == 'docs':
			out = {}
			for fn in data['fns']:
				try:
					out[fn] = str(getattr(system.tag, fn).__doc__)[:600]
				except Exception, e:
					out[fn] = 'ERR ' + str(e)
			return ok({'docs': out})
		if action == 'dirTag':
			return ok({'tag': [str(x) for x in dir(system.tag)]})
		if action == 'getConfigVariants':
			variants = data['variants']
			out = {}
			for v in variants:
				try:
					t = system.tag.getConfiguration([v], False)
					out[v] = {'ok': True, 'len': len(t), 'first': str(t[0])[:200] if t and t[0] else None}
				except java.lang.Throwable, jt:
					out[v] = {'ok': False, 'err': str(jt)[:150]}
			return ok({'variants': out})

		if action == 'getConfig':
			import java.lang.Throwable
			try:
				tags = system.tag.getConfiguration(data['tagPath'], False)
			except java.lang.Throwable, jt:
				return err(500, 'getConfiguration java error: ' + str(jt))
			if tags and tags[0]:
				return ok({'config': jv(tags[0])})
			return err(404, 'not found: ' + data['tagPath'])

		if action == 'configure':
			base = data.get('basePath', '[default]')
			result = system.tag.configure(base, data['tags'], data.get('editMode', 'm'))
			return ok({'results': [str(x) for x in result]})

		if action == 'deleteTags':
			system.tag.deleteTags(data['tagPaths'])
			return ok({'deleted': len(data['tagPaths'])})

		if action == 'listUDTTypes':
			prov = data.get('provider', 'default')
			res = system.tag.browse('[' + prov + ']_types_', {})
			out = []
			for r in res.getResults():
				try:
					out.append({'name': str(r['name']), 'fullPath': str(r['fullPath'])})
				except:
					out.append({'name': str(r.name), 'fullPath': str(r.fullPath)})
			return ok({'types': out})

		if action == 'getUDTDefinition':
			tags = system.tag.getConfiguration(data['udtPath'], True)
			if tags and tags[0]:
				return ok({'definition': jv(tags[0])})
			return err(404, 'udt not found')

		if action == 'activeAlarms':
			alarms = system.alarm.queryStatus()
			out = []
			for a in alarms:
				out.append({'eventId': str(a.eventId), 'source': str(a.source), 'priority': str(a.priority), 'state': str(a.state), 'name': str(a.name) if hasattr(a, 'name') else None})
			return ok({'results': out, 'count': len(out)})

		if action == 'alarmHistory':
			from java.util import Date
			entries = system.alarm.queryJournal(startDate=None, endDate=None)
			out = []
			count = 0
			for e in entries:
				count += 1
				if count <= 5:
					row = {}
					try:
						row['eventData'] = str(e.eventData)
						row['source'] = str(e.source)
						row['priority'] = str(e.priority)
					except:
						row = {'str': str(e)[:100]}
					out.append(row)
			return ok({'total': count, 'sample': out})

		if action == 'exportTags':
			out = {}
			def trial(label, fn):
				try:
					out[label] = {'ok': True, 'r': str(fn())[:600]}
				except java.lang.Throwable, jt:
					out[label] = {'ok': False, 'err': str(jt)[:250]}
				except Exception, e:
					out[label] = {'ok': False, 'err': 'py:' + str(e)[:250]}
			for sig in [
				('kw_paths_prov', lambda: system.tag.exportTags(tagPaths=['[default]P5'], tagProviders=['default'])),
				('kw_paths', lambda: system.tag.exportTags(tagPaths=['[default]P5'])),
				('pos_list_list', lambda: system.tag.exportTags(['[default]P5'], ['default'])),
			]:
				trial(sig[0], sig[1])
			return ok({'trials': out})
		if action == 'importTags':
			r = system.tag.importTags(data['payload'], data.get('provider', 'default'), data.get('collisionPolicy', 'o'))
			return ok({'result': str(r)})
		if action == 'browseHist':
			out = {}
			def trial(label, fn):
				try:
					out[label] = {'ok': True, 'r': str(fn())[:400]}
				except java.lang.Throwable, jt:
					out[label] = {'ok': False, 'err': str(jt)[:250]}
				except Exception, e:
					out[label] = {'ok': False, 'err': 'py:' + str(e)[:250]}
			trial('browseHistoricalTags', lambda: system.tag.browseHistoricalTags('[p5hist]'))
			trial('browseHist_noprovider', lambda: system.tag.browseHistoricalTags())
			trial('queryDensity', lambda: system.tag.queryTagDensity(['[default]H1']))
			return ok({'trials': out})
		if action == 'tagHistory':
			from java.util import Date
			import time as _t
			paths = data['paths']
			kw = {'paths': paths, 'aggregationMode': 'LastValue', 'returnSize': data.get('returnSize', 10)}
			if data.get('windowMinutes'):
				kw['startDate'] = Date(long((_t.time() - data['windowMinutes']*60) * 1000))
				kw['endDate'] = Date(long(_t.time() * 1000))
			if data.get('raw'):
				kw.pop('aggregationMode', None)
				kw.pop('returnSize', None)
				kw['intervalHours'] = 1.0 / 60.0
			ds = system.tag.queryTagHistory(**kw)
			cols = []
			for c in range(ds.columnCount):
				col = {'name': str(ds.getColumnName(c)), 'rows': []}
				for r in range(ds.rowCount):
					col['rows'].append(str(ds.getValueAt(r, c)))
				cols.append(col)
			return ok({'columns': cols, 'rowCount': ds.rowCount})

		if action == 'exportTags':
			payload = system.tag.exportToPayload(data['paths'], data.get('provider', 'default'))
			return ok({'payloadType': str(type(payload)), 'payload': payload})

		if action == 'importTags':
			payload = data['payload']
			try:
				system.tag.importFromPayload(payload, data.get('provider', 'default'), data.get('mode', 'o'))
				return ok({'imported': True})
			except Exception as ie:
				return ok({'imported': False, 'err': str(ie)})

		return err(400, 'unknown action: ' + str(action))
	except:
		return err(500, 'probe error', traceback.format_exc())

def doPost(request, session):
	"""Read/write tags for E2E testing. Auto-creates missing tags.

	POST body: { "writes": [{"path": "[default]Tag/Path", "value": "someValue"}, ...] }
	Response:  { "results": [{"path": "...", "success": true}, ...] }

	Also supports reading: { "reads": ["[default]Tag/Path", ...] }
	Response:  { "values": {"[default]Tag/Path": {"value": ..., "quality": ...}} }

	Mirror OPC tags to memory (for testing without PLC):
	  { "mirror": {"source": "[provider]Path/To/Folder", "dest": "[provider]Path/To/Folder_MEM"} }
	  Response: { "mirror": {"success": true, "dest": "..."} }

	Delete mirrored tags:
	  { "deleteTags": "[provider]Path/To/Folder_MEM" }
	  Response: { "deleteTags": {"success": true} }

	Call a gateway script function (for testing real scripts end-to-end):
	  { "script": {"path": "core.mes.mashing.enzyme_entry.validate_and_submit", "args": [1, 1, 5.0, "gal", "LOT-001"]} }
	  Response: { "script": {"success": true, "result": {...}} }
	"""
	# Nested (8.3 WebDev keeps ONLY doPost - module-level defs vanish;
	# live-pinned 8.3.6).
	def _ensureTag(tagPath, value):
		"""Create a memory tag at the given path if it doesn't exist.

		Parses the path to extract provider, folder structure, and tag name.
		Creates the full folder hierarchy and a memory tag of the appropriate type.
		"""
		# Parse "[provider]Folder/SubFolder/TagName"
		providerEnd = tagPath.index(']') + 1
		provider = tagPath[1:providerEnd - 1]  # e.g. "default"
		remainingPath = tagPath[providerEnd:]   # e.g. "Changeover/cooker/state"

		parts = remainingPath.split('/')
		tagName = parts[-1]
		folderParts = parts[:-1]

		# Determine tag data type from value
		dataType = _inferDataType(value)

		# Build the base path for system.tag.configure
		basePath = tagPath[:tagPath.rfind('/')]

		# Create tag configuration
		tagConfig = {
			'name': tagName,
			'tagType': 'AtomicTag',
			'dataType': dataType,
			'value': value
		}

		# system.tag.configure creates parent folders automatically
		result = system.tag.configure(basePath, [tagConfig], 'o')  # 'o' = overwrite if exists
		return result

	def _inferDataType(value):
		"""Infer Ignition tag data type from a Python value."""
		if isinstance(value, bool):
			return 'Boolean'
		elif isinstance(value, int):
			return 'Int4'
		elif isinstance(value, float):
			return 'Float8'
		else:
			return 'String'
	import json

	data = request['data']
	if isinstance(data, (str, unicode)):
		data = system.util.jsonDecode(data)

	response = {}

	# Handle mirror - clone OPC subtree as memory tags.
	# NOTE: This requires a `convert_to_memory_tags(source, dest)` function in
	# your project's script library.  The function should browse the source OPC
	# tag tree and recreate it under dest with valueSource="memory".
	# If you don't need mirror functionality, you can safely remove this block.
	# Mirror tags - copies OPC/device tags to writable memory tags for testing.
	# Uncomment and implement when your project has a convert_to_memory_tags function.
	# if 'mirror' in data:
	# 	mirrorCfg = data['mirror']
	# 	source = mirrorCfg['source']
	# 	dest = mirrorCfg.get('dest', source + '_MEM')
	# 	try:
	# 		your_project.tools.conversions.convert_to_memory_tags(source, dest)
	# 		response['mirror'] = {'success': True, 'dest': dest}
	# 	except Exception:
	# 		import traceback
	# 		response['mirror'] = {'success': False, 'error': traceback.format_exc()}

	# Handle deleteTags - remove a tag tree (for cleanup after mirror)
	if 'deleteTags' in data:
		delPath = data['deleteTags']
		try:
			system.tag.deleteTags([delPath])
			response['deleteTags'] = {'success': True}
		except Exception:
			import traceback
			response['deleteTags'] = {'success': False, 'error': traceback.format_exc()}

	# Handle script - call a gateway script function by dotted path
	if 'script' in data:
		scriptCfg = data['script']
		scriptPath = scriptCfg['path']
		scriptArgs = scriptCfg.get('args', [])
		scriptKwargs = scriptCfg.get('kwargs', {})
		try:
			# Resolve the function from dotted path, e.g. "core.mes.mashing.enzyme_entry.validate_and_submit"
			parts = scriptPath.split('.')
			funcName = parts[-1]
			modulePath = '.'.join(parts[:-1])
			# Navigate the module hierarchy
			mod = __import__(modulePath)
			for comp in parts[1:-1]:
				mod = getattr(mod, comp)
			func = getattr(mod, funcName)
			result = func(*scriptArgs, **scriptKwargs)
			response['script'] = {'success': True, 'result': result}
		except Exception:
			import traceback
			response['script'] = {'success': False, 'error': traceback.format_exc()}

	# Handle writes - auto-create tags if they don't exist
	if 'writes' in data:
		paths = []
		values = []
		for w in data['writes']:
			paths.append(w['path'])
			values.append(w['value'])

		# First attempt: write directly
		writeResults = system.tag.writeBlocking(paths, values)
		results = []
		needCreate = []

		for i, qc in enumerate(writeResults):
			if qc.isGood():
				results.append({
					'path': paths[i],
					'success': True,
					'quality': str(qc)
				})
			else:
				# Review round: auto-create ONLY on a not-found quality -
				# a read-only, OPC-owned, permission-denied, or unhealthy
				# tag must surface its real quality, not silently gain a
				# shadow memory twin.
				code = str(getattr(qc, 'code', '') or qc)
				if 'NotFound' in code or 'BadTag' in code:
					needCreate.append(i)
					results.append(None)  # placeholder
				else:
					results.append({
						'path': paths[i],
						'success': False,
						'quality': str(qc)
					})

		# Create missing tags and retry writes
		if needCreate:
			for idx in needCreate:
				tagPath = paths[idx]
				tagValue = values[idx]
				_ensureTag(tagPath, tagValue)

			# Retry writes for created tags
			retryPaths = [paths[i] for i in needCreate]
			retryValues = [values[i] for i in needCreate]
			retryResults = system.tag.writeBlocking(retryPaths, retryValues)

			for j, idx in enumerate(needCreate):
				qc = retryResults[j]
				results[idx] = {
					'path': paths[idx],
					'success': qc.isGood(),
					'quality': str(qc)
				}

		response['results'] = results

	# Handle importTags - import tag JSON file into the provider via system.tag.configure
	if 'importTags' in data:
		importCfg = data['importTags']
		filePath = importCfg['filePath']
		basePath = importCfg['basePath']
		tagName = importCfg.get('tagName', None)
		collisionPolicy = importCfg.get('collisionPolicy', 'o')
		try:
			import os
			projectRoot = system.util.getProjectName()
			projectRoot = os.path.join(
				system.tag.readBlocking(["[System]Gateway/SystemProperties/DataDirectory"])[0].value or "/usr/local/bin/ignition/data",
				"projects", projectRoot
			)
			fullPath = os.path.normpath(os.path.join(projectRoot, filePath))
			# Prevent path traversal outside the project directory
			if not fullPath.startswith(projectRoot + os.sep):
				response['importTags'] = {'success': False, 'error': 'Invalid filePath: path traversal detected'}
				return {'json': response}
			with open(fullPath, 'r') as f:
				tagJson = f.read()
			tagObj = system.util.jsonDecode(tagJson)
			if tagName:
				tagObj['name'] = tagName
			results = system.tag.configure(basePath, [tagObj], collisionPolicy)
			response['importTags'] = {'success': True, 'results': str(results)}
		except Exception:
			import traceback
			response['importTags'] = {'success': False, 'error': traceback.format_exc()}

	# Handle reads
	if 'reads' in data:
		tagPaths = data['reads']
		tagValues = system.tag.readBlocking(tagPaths)
		values = {}
		for i, qv in enumerate(tagValues):
			values[tagPaths[i]] = {
				'value': qv.value,
				'quality': str(qv.quality),
				'good': qv.quality.isGood()
			}
		response['values'] = values

	return {'json': response}

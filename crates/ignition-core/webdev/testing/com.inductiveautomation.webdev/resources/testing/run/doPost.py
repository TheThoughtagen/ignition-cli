def doPost(request, session):
	"""Run tests and return JSON results.

	POST body (JSON, optional):
		{}                              - run everything
		{"discover": true}             - list discovered test modules
		{"module": "..."}              - run one module
		{"package": "..."}             - run one package prefix
		{"format": "json"|"junit"|"text"}

	Returns:
		200 if all tests pass
		207 if there are failures or errors
		500 on runner error

	8.3 WebDev platform contract (live-pinned on 8.3.6): the route
	file's module namespace is NOT kept - ONLY doPost survives - so
	every helper lives NESTED inside this function, and the file
	carries ZERO module-level code. The route context also has NO
	project-script classloader, so the framework is seeded into
	sys.modules from the gateway filesystem (the Designer/Script
	Console path - native imports - is untouched).
	"""
	import json
	import sys
	import types
	import traceback
	from java.io import File

	# Deploy-time marker: this route's own project (substituted by
	# `ign webdev deploy --with-testing` / adopt --testing).
	PROJECT_NAME = "__IGN_CLI_PROJECT__"

	def data_dirs():
		# Candidate gateway DATA directories, best first. The 8.1
		# [System]Gateway/SystemProperties/DataDirectory tag is
		# Bad_NotFound on live 8.3.6 (the System tree was
		# reorganized), so the ladder continues with the process CWD
		# (the install root on stock installs and the docker image -
		# data/ beneath it), the CWD itself when it IS the data dir,
		# then stock defaults.
		dirs = []
		try:
			qv = system.tag.readBlocking(
				["[System]Gateway/SystemProperties/DataDirectory"])[0]
			if qv.quality.isGood() and qv.value:
				dirs.append(str(qv.value))
		except Exception:
			pass
		cwd_data = File(".", "data")
		if cwd_data.isDirectory():
			dirs.append(cwd_data.getAbsolutePath())
		if File(".", "projects").isDirectory():
			dirs.append(File(".").getAbsolutePath())
		dirs.append("/usr/local/bin/ignition/data")
		dirs.append("C:/Program Files/Inductive Automation/Ignition/data")
		return dirs

	def seed_testing():
		# Build the `testing` package in sys.modules from the on-disk
		# framework sources (exec'd into real module objects). Scoped
		# to THIS route's project first (the deploy-time marker --
		# review round: an unscoped scan could load another project's
		# framework and execute ITS tests); the scan remains only as
		# a fallback for hand-copied bundles without the marker.
		if "testing.runner" in sys.modules:
			return
		root = None
		for data_dir in data_dirs():
			scoped = File(
				File(File(data_dir, "projects"), PROJECT_NAME),
				"ignition/script-python/resources/testing")
			if scoped.isDirectory():
				root = scoped
				break
		if root is None:
			for data_dir in data_dirs():
				projects_root = File(data_dir, "projects")
				if not projects_root.isDirectory():
					continue
				for project_dir in (projects_root.listFiles() or []):
					if not project_dir.isDirectory():
						continue
					candidate = File(
						project_dir, "ignition/script-python/resources/testing")
					if candidate.isDirectory():
						root = candidate
						break
				if root is not None:
					break
		if root is None:
			raise ImportError(
				"testing framework not found on the gateway filesystem - "
				"deploy the testing bundle first")
		package = types.ModuleType("testing")
		package.__path__ = []
		sys.modules["testing"] = package
		for module_name in ("assertions", "decorators", "helpers", "reporter", "runner"):
			code_file = File(File(root, module_name), "code.py")
			source = open(code_file.getAbsolutePath()).read()
			module = types.ModuleType("testing." + module_name)
			sys.modules["testing." + module_name] = module
			setattr(package, module_name, module)
			exec source in module.__dict__

	def get_param(params, key):
		# Extract one query param value from Ignition WebDev params
		# (Java String[] arrays - plain string indexing would grab the
		# first CHARACTER, so the type is checked).
		val = params.get(key)
		if val is None:
			return None
		s = str(val)
		try:
			if hasattr(val, 'getClass') and val.getClass().isArray():
				from java.lang.reflect import Array
				if Array.getLength(val) > 0:
					return str(Array.get(val, 0))
				return None
		except Exception:
			pass
		if isinstance(val, (list, tuple)):
			return str(val[0]) if val else None
		return s

	seed_testing()
	import testing.runner
	import testing.reporter

	module = None
	package = ""
	fmt = "json"
	discover = False

	params = request.get("params", {})
	if params:
		discover = get_param(params, "discover") in ("true", "1", True)
		module = get_param(params, "module")
		package = get_param(params, "package") or ""
		fmt = get_param(params, "format") or "json"

	body = request.get("data", None)
	if body is not None:
		try:
			if hasattr(body, "read"):
				body = body.read()
			if isinstance(body, (str, unicode)):
				body = json.loads(body)
			body_data = body
			if isinstance(body_data, dict):
				discover = body_data.get("discover", discover)
				module = body_data.get("module", module)
				package = body_data.get("package", package)
				fmt = body_data.get("format", fmt)
		except Exception:
			pass  # Fall through to query string params if body is not valid JSON

	if discover:
		modules = testing.runner._discover_test_modules()
		return {"json": {
			"discovered_modules": modules,
			"count": len(modules),
		}}

	try:
		if module:
			results = testing.runner.run_module(module)
			# Wrap single module in the run_all structure
			results = {
				"passed": results["passed"],
				"failed": results["failed"],
				"skipped": results["skipped"],
				"errors": results["errors"],
				"total": results["passed"] + results["failed"] + results["skipped"] + results["errors"],
				"duration_ms": results["duration_ms"],
				"modules": [results],
			}
		else:
			results = testing.runner.run_all(base_package=package)

		if fmt == "junit":
			xml = testing.reporter.to_junit_xml(results)
			return {
				"html": xml,
				"content-type": "application/xml",
			}
		elif fmt == "text":
			text = testing.reporter.to_console(results)
			return {
				"html": "<pre>%s</pre>" % text,
				"content-type": "text/plain",
			}

		# Default JSON
		status = 200 if results["failed"] == 0 and results["errors"] == 0 else 207
		request['servletResponse'].setStatus(status)
		return {"json": results}

	except Exception as e:
		# Review round: an exception BEFORE the normal status update
		# must not ride the default 200 (status-based clients would
		# read a runner blowup as success).
		try:
			request['servletResponse'].setStatus(500)
		except Exception:
			pass
		return {"json": {
			"error": str(e),
			"traceback": traceback.format_exc(),
		}}

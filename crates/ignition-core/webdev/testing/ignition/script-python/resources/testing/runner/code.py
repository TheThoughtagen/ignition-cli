"""
Test runner: discovers and executes @test-decorated functions.

Location: testing.runner

Usage from Script Console:
	print testing.runner.run_all()
	print testing.runner.run_module("core.mes.changeover.__tests__")

Usage from WebDev:
	results = testing.runner.run_all()
	return {'json': results}
"""

import time
import traceback

from java.io import File


# ---------------------------------------------------------------------------
# Configuration - PROJECT_NAME is only the FALLBACK for the runtime
# data-directory probe; ign's deploy substitutes the deploy project.
# ---------------------------------------------------------------------------

PROJECT_NAME = "__IGN_CLI_PROJECT__"


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def run_all(base_package=""):
	"""Discover and run all test modules under the script library.

	Args:
		base_package: Dotted package prefix to filter (e.g. "core.mes").
		              Empty string means run everything.

	Returns:
		dict: {passed, failed, skipped, errors, duration_ms, results: [...]}
	"""
	modules = _discover_test_modules()

	if base_package:
		modules = [m for m in modules if m.startswith(base_package)]

	all_results = []
	t0 = time.time()

	for module_path in sorted(modules):
		module_results = run_module(module_path)
		all_results.append(module_results)

	elapsed = int((time.time() - t0) * 1000)

	passed = sum(m["passed"] for m in all_results)
	failed = sum(m["failed"] for m in all_results)
	skipped = sum(m["skipped"] for m in all_results)
	errors = sum(m["errors"] for m in all_results)

	return {
		"passed": passed,
		"failed": failed,
		"skipped": skipped,
		"errors": errors,
		"total": passed + failed + skipped + errors,
		"duration_ms": elapsed,
		"modules": all_results,
	}


def run_module(module_path):
	"""Import a specific test module and run all @test functions in it.

	Args:
		module_path: Dotted module path (e.g. "core.mes.changeover.__tests__")

	Returns:
		dict: {module, passed, failed, skipped, errors, duration_ms, results: [...]}
	"""
	result = {
		"module": module_path,
		"passed": 0,
		"failed": 0,
		"skipped": 0,
		"errors": 0,
		"duration_ms": 0,
		"results": [],
	}

	t0 = time.time()

	# Import the module
	try:
		mod = _import_module(module_path)
	except Exception as e:
		result["errors"] = 1
		result["results"].append({
			"name": "(module import)",
			"status": "error",
			"message": "Failed to import %s: %s" % (module_path, str(e)),
			"traceback": traceback.format_exc(),
			"duration_ms": 0,
		})
		result["duration_ms"] = int((time.time() - t0) * 1000)
		return result

	# Find setup/teardown and test functions
	setup_fn = None
	teardown_fn = None
	test_fns = []

	for name in dir(mod):
		obj = getattr(mod, name)
		if not callable(obj):
			continue
		if getattr(obj, "_is_setup", False):
			setup_fn = obj
		elif getattr(obj, "_is_teardown", False):
			teardown_fn = obj
		elif getattr(obj, "_is_test", False):
			test_fns.append((name, obj))

	# Sort tests by name for deterministic ordering
	test_fns.sort(key=lambda pair: pair[0])

	# Run setup
	if setup_fn is not None:
		try:
			setup_fn()
		except Exception as e:
			result["errors"] = 1
			result["results"].append({
				"name": "(setup)",
				"status": "error",
				"message": "Setup failed: %s" % str(e),
				"traceback": traceback.format_exc(),
				"duration_ms": 0,
			})
			result["duration_ms"] = int((time.time() - t0) * 1000)
			return result

	# Run tests
	for name, func in test_fns:
		test_result = _run_single_test(name, func)
		result["results"].append(test_result)
		status = test_result["status"]
		if status == "passed":
			result["passed"] += 1
		elif status == "failed":
			result["failed"] += 1
		elif status == "skipped":
			result["skipped"] += 1
		elif status == "error":
			result["errors"] += 1

	# Run teardown
	if teardown_fn is not None:
		try:
			teardown_fn()
		except Exception as e:
			result["results"].append({
				"name": "(teardown)",
				"status": "error",
				"message": "Teardown failed: %s" % str(e),
				"traceback": traceback.format_exc(),
				"duration_ms": 0,
			})
			result["errors"] += 1

	result["duration_ms"] = int((time.time() - t0) * 1000)
	return result


# ---------------------------------------------------------------------------
# Discovery
# ---------------------------------------------------------------------------

def _data_dirs():
	"""Candidate gateway DATA directories, best first (shared ladder
	with the testing/run route's loader - duplicated per the
	self-contained convention).

	8.1 exposed the data dir as the [System]Gateway/SystemProperties/
	DataDirectory tag - on live 8.3.6 that path is Bad_NotFound (the
	System tag tree was reorganized), so the ladder continues: the
	process CWD (the install root on stock installs and the docker
	image - data/ sits under it), the CWD itself when it IS the data
	dir (projects/ directly beneath), then stock defaults.
	"""
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


def _script_bases():
	"""Every project's script-python base directory (8.3 resources
	container when present, 8.1 bare collection otherwise).

	Bases cover ALL projects on the gateway - tests are found in the
	current project and any parent/sibling projects, matching how
	project inheritance makes scripts available at runtime.

	Returns:
		list[str]: Absolute script-python base paths (module names are
		relative to THESE).
	"""
	bases = []

	for data_dir in _data_dirs():
		gateway_projects_root = File(data_dir, "projects")
		if not gateway_projects_root.isDirectory():
			continue
		for project_dir in (gateway_projects_root.listFiles() or []):
			if not project_dir.isDirectory():
				continue
			script_path = File(project_dir, "ignition/script-python")
			if not script_path.exists():
				continue
			# 8.1 layout: modules sit DIRECTLY under script-python.
			# 8.3 layout: modules sit under the script-python/resources
			# container (the on-disk spelling of the resource-folder
			# layout - live-verified 8.3.6). Base the walk - and thus
			# the computed dotted module paths - on whichever exists.
			resources_path = File(script_path, "resources")
			base = resources_path if resources_path.exists() else script_path
			bases.append(base.getAbsolutePath())

	return bases


def _discover_test_modules():
	"""Walk the script library filesystem to find __tests__/code.py modules.

	Looks for both __tests__ (lowercase, new convention) and __TESTS__
	(uppercase, legacy convention) directories containing code.py.

	Returns:
		list[str]: Dotted module paths (e.g. ["core.mes.changeover.__tests__"])
	"""
	modules = []
	seen = set()

	for base_dir in _script_bases():
		base_file = File(base_dir)
		if not base_file.exists():
			continue

		_walk_for_tests(base_file, base_dir, modules, seen)

	return sorted(modules)


def _walk_for_tests(directory, base_dir, modules, seen):
	"""Recursively walk directory to find __tests__/code.py files.

	Args:
		directory: java.io.File directory to search
		base_dir: Root script-python path for computing module names
		modules: List to append discovered module paths to
		seen: Set of already-seen module paths (avoids duplicates)
	"""
	children = directory.listFiles()
	if children is None:
		return

	for child in children:
		if not child.isDirectory():
			continue

		name = child.getName()

		# Check if this is a test directory
		if name in ("__tests__", "__TESTS__"):
			code_file = File(child, "code.py")
			if code_file.exists():
				# Convert filesystem path to dotted module path
				rel_path = child.getAbsolutePath()[len(base_dir) + 1:]
				module_path = rel_path.replace("/", ".").replace("\\", ".")
				if module_path not in seen:
					seen.add(module_path)
					modules.append(module_path)
		else:
			# Recurse into subdirectories
			_walk_for_tests(child, base_dir, modules, seen)


# ---------------------------------------------------------------------------
# Import & Execution
# ---------------------------------------------------------------------------

def _import_module(module_path):
	"""Import a dotted module path and return the module object.

	Project-script contexts (Designer, Script Console) import natively.
	8.3 WebDev route contexts carry NO project-script classloader
	(live-pinned: `__import__` raises ImportError for every project
	package) - there the FILESYSTEM fallback resolves the module's
	code.py under the discovered bases, execs it into a fresh module,
	and registers it in sys.modules (the same seeding the routes do
	for the framework itself).

	Args:
		module_path: Dotted module path

	Returns:
		module object
	"""
	parts = module_path.split(".")
	try:
		mod = __import__(module_path)
		# __import__ returns the top-level module; traverse to the leaf
		for part in parts[1:]:
			mod = getattr(mod, part)
		return mod
	except ImportError:
		import sys
		import types
		if module_path in sys.modules:
			return sys.modules[module_path]
		rel_path = "/".join(parts)
		for base_dir in _script_bases():
			code_file = File(File(base_dir, rel_path), "code.py")
			if code_file.exists():
				source = open(code_file.getAbsolutePath()).read()
				module = types.ModuleType(module_path)
				try:
					exec(source, module.__dict__)
				except Exception:
					# Never cache a partial module (review round): a
					# half-executed module in sys.modules would satisfy
					# later runs WITHOUT executing its source.
					sys.modules.pop(module_path, None)
					raise
				sys.modules[module_path] = module
				# Attach to the parent package so `from pkg import mod`
				# resolves for later imports too.
				if len(parts) > 1:
					parent = sys.modules.get(".".join(parts[:-1]))
					if parent is not None:
						setattr(parent, parts[-1], module)
				return module
		raise


def _run_single_test(name, func):
	"""Execute a single test function and return its result.

	Handles @skip, @expected_error decorators, and captures exceptions.

	Args:
		name: Test function name
		func: The test function

	Returns:
		dict: {name, status, message, traceback, duration_ms}
	"""
	# Check for skip
	if getattr(func, "_skip", False):
		reason = getattr(func, "_skip_reason", "")
		return {
			"name": name,
			"status": "skipped",
			"message": reason,
			"traceback": None,
			"duration_ms": 0,
		}

	expected_error = getattr(func, "_expected_error", None)

	t0 = time.time()
	try:
		func()
		elapsed = int((time.time() - t0) * 1000)

		if expected_error is not None:
			# Expected an exception but none was raised
			return {
				"name": name,
				"status": "failed",
				"message": "Expected %s to be raised" % expected_error.__name__,
				"traceback": None,
				"duration_ms": elapsed,
			}

		return {
			"name": name,
			"status": "passed",
			"message": None,
			"traceback": None,
			"duration_ms": elapsed,
		}

	except Exception as e:
		elapsed = int((time.time() - t0) * 1000)

		if expected_error is not None and isinstance(e, expected_error):
			return {
				"name": name,
				"status": "passed",
				"message": "Raised expected %s" % expected_error.__name__,
				"traceback": None,
				"duration_ms": elapsed,
			}

		# Determine if this is an assertion failure or an unexpected error
		from testing.assertions import TestAssertionError
		if isinstance(e, (TestAssertionError, AssertionError)):
			status = "failed"
		else:
			status = "error"

		return {
			"name": name,
			"status": status,
			"message": str(e),
			"traceback": traceback.format_exc(),
			"duration_ms": elapsed,
		}

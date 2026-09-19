"""
Test result formatters: JSON, JUnit XML, and console output.

Location: testing.reporter

Usage:
	results = testing.runner.run_all()
	print testing.reporter.to_console(results)
	xml = testing.reporter.to_junit_xml(results)
	json_str = testing.reporter.to_json(results)
"""

import json
import time


def to_json(results):
	"""Serialize test results to a JSON string.

	Args:
		results: Dict from testing.runner.run_all() or run_module()

	Returns:
		str: JSON string
	"""
	return json.dumps(results, indent=2)


def to_console(results):
	"""Format test results as human-readable text for the Script Console.

	Args:
		results: Dict from testing.runner.run_all()

	Returns:
		str: Formatted text output
	"""
	lines = []
	lines.append("=" * 60)
	lines.append("TEST RESULTS")
	lines.append("=" * 60)

	modules = results.get("modules", [])
	if not modules:
		# Single module result (from run_module)
		modules = [results]

	for mod in modules:
		module_name = mod.get("module", "unknown")
		lines.append("")
		lines.append("--- %s ---" % module_name)

		for r in mod.get("results", []):
			status = r["status"].upper()
			name = r["name"]

			if status == "PASSED":
				marker = "  PASS"
			elif status == "FAILED":
				marker = "  FAIL"
			elif status == "SKIPPED":
				marker = "  SKIP"
			else:
				marker = " ERROR"

			line = "%s  %s" % (marker, name)
			if r.get("duration_ms", 0) > 0:
				line += "  (%dms)" % r["duration_ms"]
			lines.append(line)

			if r.get("message") and status in ("FAILED", "ERROR"):
				lines.append("         %s" % r["message"])

	lines.append("")
	lines.append("=" * 60)
	lines.append(
		"Total: %d  Passed: %d  Failed: %d  Skipped: %d  Errors: %d  (%dms)" % (
			results.get("total", 0),
			results.get("passed", 0),
			results.get("failed", 0),
			results.get("skipped", 0),
			results.get("errors", 0),
			results.get("duration_ms", 0),
		)
	)
	lines.append("=" * 60)

	return "\n".join(lines)


def to_junit_xml(results):
	"""Format test results as JUnit XML for CI integration.

	Args:
		results: Dict from testing.runner.run_all()

	Returns:
		str: JUnit XML string
	"""
	modules = results.get("modules", [])
	if not modules:
		modules = [results]

	xml_parts = ['<?xml version="1.0" encoding="UTF-8"?>']
	xml_parts.append('<testsuites tests="%d" failures="%d" errors="%d" skipped="%d" time="%.3f">' % (
		results.get("total", 0),
		results.get("failed", 0),
		results.get("errors", 0),
		results.get("skipped", 0),
		results.get("duration_ms", 0) / 1000.0,
	))

	for mod in modules:
		module_name = mod.get("module", "unknown")
		test_results = mod.get("results", [])

		mod_failures = mod.get("failed", 0)
		mod_errors = mod.get("errors", 0)
		mod_skipped = mod.get("skipped", 0)
		mod_tests = len(test_results)
		mod_time = mod.get("duration_ms", 0) / 1000.0

		xml_parts.append(
			'  <testsuite name="%s" tests="%d" failures="%d" errors="%d" skipped="%d" time="%.3f">'
			% (_escape_xml(module_name), mod_tests, mod_failures, mod_errors, mod_skipped, mod_time)
		)

		for r in test_results:
			name = r.get("name", "unknown")
			t = r.get("duration_ms", 0) / 1000.0
			status = r.get("status", "error")

			xml_parts.append(
				'    <testcase name="%s" classname="%s" time="%.3f">'
				% (_escape_xml(name), _escape_xml(module_name), t)
			)

			if status == "failed":
				msg = _escape_xml(r.get("message", ""))
				tb = _escape_xml(r.get("traceback", "") or "")
				xml_parts.append('      <failure message="%s">%s</failure>' % (msg, tb))
			elif status == "error":
				msg = _escape_xml(r.get("message", ""))
				tb = _escape_xml(r.get("traceback", "") or "")
				xml_parts.append('      <error message="%s">%s</error>' % (msg, tb))
			elif status == "skipped":
				reason = _escape_xml(r.get("message", ""))
				xml_parts.append('      <skipped message="%s" />' % reason)

			xml_parts.append('    </testcase>')

		xml_parts.append('  </testsuite>')

	xml_parts.append('</testsuites>')

	return "\n".join(xml_parts)


def _escape_xml(text):
	"""Escape special XML characters."""
	if text is None:
		return ""
	text = str(text)
	# Review round: XML 1.0 forbids most control characters (only
	# \t \n \r are legal below 0x20) -- strip the rest or the
	# document is malformed, not just escaped.
	text = "".join(
		ch for ch in text
		if ch in "\t\n\r" or ord(ch) >= 0x20)
	text = text.replace("&", "&amp;")
	text = text.replace("<", "&lt;")
	text = text.replace(">", "&gt;")
	text = text.replace('"', "&quot;")
	return text

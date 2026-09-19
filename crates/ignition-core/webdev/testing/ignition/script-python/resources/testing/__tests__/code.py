"""
Framework self-test - the permanent smoke sentinel.

Ships with the testing bundle so `?discover=true` ALWAYS lists at
least one module and `run_all` proves the whole chain live (the
empty-suite trap: a green deploy with zero discoverable modules is
indistinguishable from a broken discovery walk - this module makes
the distinction observable).

Location: testing.__tests__
"""

from testing.assertions import assert_equal
from testing.decorators import test


@test
def _framework_imports():
	"""Every framework module imports and exposes its public API."""
	from testing import assertions, decorators, helpers, reporter, runner

	assert_equal(callable(runner.run_all), True, "runner.run_all callable")
	assert_equal(callable(runner.run_module), True, "runner.run_module callable")
	assert_equal(callable(reporter.to_junit_xml), True, "reporter.to_junit_xml callable")
	assert_equal(callable(helpers.write_dataset_tag), True, "helpers.write_dataset_tag callable")
	assert_equal(assertions.TestAssertionError is not None, True, "TestAssertionError present")


@test
def _assertion_failure_path():
	"""The failed-assertion path classifies as failed, not error."""
	from testing.assertions import TestAssertionError

	raised = False
	try:
		assert_equal(1, 2, "intentional mismatch - the self-test proving the fail path")
	except TestAssertionError:
		raised = True
	assert_equal(raised, True, "assert_equal raises TestAssertionError on mismatch")

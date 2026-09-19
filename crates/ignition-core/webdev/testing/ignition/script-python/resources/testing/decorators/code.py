"""
Test decorators for marking functions as tests.

Location: testing.decorators

Usage:
	from testing.decorators import test, skip, setup, teardown, expected_error

	@test
	def my_test():
		assert_equal(1 + 1, 2)

	@skip("not implemented yet")
	@test
	def future_test():
		pass

	@expected_error(ValueError)
	@test
	def test_bad_input():
		int("abc")
"""


def test(func):
	"""Mark a function as a test case.

	Sets _is_test = True on the function so the runner can discover it.
	"""
	func._is_test = True
	return func


def skip(reason=""):
	"""Mark a test to be skipped during execution.

	Args:
		reason: Optional explanation for why the test is skipped

	Usage:
		@skip("waiting on tag config")
		@test
		def test_something():
			pass
	"""
	def decorator(func):
		func._skip = True
		func._skip_reason = reason
		return func
	return decorator


def setup(func):
	"""Mark a function as module-level setup (runs before all tests)."""
	func._is_setup = True
	return func


def teardown(func):
	"""Mark a function as module-level teardown (runs after all tests)."""
	func._is_teardown = True
	return func


def expected_error(exception_type):
	"""Mark a test that should raise a specific exception type.

	The test passes if the expected exception is raised, fails otherwise.

	Args:
		exception_type: The exception class expected (e.g. ValueError, KeyError)

	Usage:
		@expected_error(ValueError)
		@test
		def test_bad_parse():
			int("not a number")
	"""
	def decorator(func):
		func._expected_error = exception_type
		return func
	return decorator

"""
Test assertion functions with descriptive failure messages.

Location: testing.assertions

Usage:
	from testing.assertions import assert_equal, assert_true, assert_raises

	assert_equal(actual, expected)
	assert_true(value, "should be truthy")
	assert_raises(lambda: int("abc"), ValueError)
"""


class TestAssertionError(AssertionError):
	"""Assertion error with structured context for test reporting."""

	def __init__(self, message, actual=None, expected=None):
		self.actual = actual
		self.expected = expected
		super(TestAssertionError, self).__init__(message)


def assert_equal(actual, expected, msg=None):
	"""Assert actual == expected."""
	if actual != expected:
		text = msg or "Expected %r but got %r" % (expected, actual)
		raise TestAssertionError(text, actual=actual, expected=expected)


def assert_not_equal(actual, expected, msg=None):
	"""Assert actual != expected."""
	if actual == expected:
		text = msg or "Expected values to differ but both are %r" % (actual,)
		raise TestAssertionError(text, actual=actual, expected=expected)


def assert_true(val, msg=None):
	"""Assert val is truthy."""
	if not val:
		text = msg or "Expected truthy value but got %r" % (val,)
		raise TestAssertionError(text, actual=val, expected=True)


def assert_false(val, msg=None):
	"""Assert val is falsy."""
	if val:
		text = msg or "Expected falsy value but got %r" % (val,)
		raise TestAssertionError(text, actual=val, expected=False)


def assert_none(val, msg=None):
	"""Assert val is None."""
	if val is not None:
		text = msg or "Expected None but got %r" % (val,)
		raise TestAssertionError(text, actual=val, expected=None)


def assert_not_none(val, msg=None):
	"""Assert val is not None."""
	if val is None:
		text = msg or "Expected a value but got None"
		raise TestAssertionError(text, actual=None, expected="not None")


def assert_close(actual, expected, tolerance=0.001, msg=None):
	"""Assert actual is within tolerance of expected (for floating point).

	Args:
		actual: Actual numeric value
		expected: Expected numeric value
		tolerance: Maximum allowed absolute difference (default 0.001)
		msg: Optional failure message
	"""
	diff = abs(actual - expected)
	if diff > tolerance:
		text = msg or "Expected %r within %s of %r (diff=%s)" % (
			actual, tolerance, expected, diff
		)
		raise TestAssertionError(text, actual=actual, expected=expected)


def assert_raises(callable_fn, exception_type, msg=None):
	"""Assert that calling callable_fn raises the given exception type.

	Args:
		callable_fn: Zero-argument callable to invoke
		exception_type: Expected exception class
		msg: Optional failure message

	Returns:
		The caught exception instance (for further inspection)
	"""
	try:
		callable_fn()
	except exception_type as e:
		return e
	except Exception as e:
		text = msg or "Expected %s but got %s: %s" % (
			exception_type.__name__, type(e).__name__, str(e)
		)
		raise TestAssertionError(text, actual=type(e).__name__, expected=exception_type.__name__)
	else:
		text = msg or "Expected %s to be raised but nothing was raised" % (
			exception_type.__name__,
		)
		raise TestAssertionError(text, actual="no exception", expected=exception_type.__name__)


def assert_tag_value(tag_path, expected, msg=None):
	"""Read a real gateway tag and assert its value equals expected.

	Args:
		tag_path: Full tag path (e.g. '[default]Path/To/Tag')
		expected: Expected value
		msg: Optional failure message
	"""
	qv = system.tag.readBlocking([tag_path])[0]
	actual = qv.value
	if actual != expected:
		text = msg or "Tag '%s' expected %r but got %r (quality=%s)" % (
			tag_path, expected, actual, qv.quality
		)
		raise TestAssertionError(text, actual=actual, expected=expected)


def assert_contains(container, item, msg=None):
	"""Assert that item is in container."""
	if item not in container:
		text = msg or "Expected %r to contain %r" % (container, item)
		raise TestAssertionError(text, actual=container, expected=item)


def assert_isinstance(obj, expected_type, msg=None):
	"""Assert that obj is an instance of expected_type."""
	if not isinstance(obj, expected_type):
		text = msg or "Expected instance of %s but got %s" % (
			expected_type.__name__, type(obj).__name__
		)
		raise TestAssertionError(text, actual=type(obj).__name__, expected=expected_type.__name__)

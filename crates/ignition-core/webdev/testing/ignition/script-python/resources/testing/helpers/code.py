"""
Helper functions callable from E2E tests via the callScript endpoint.

These bridge the gap between the testing/tags WebDev endpoint (which writes
raw JSON values) and Ignition tag types that need native objects (DataSets,
etc.).
"""
import system


def write_dataset_tag(tag_path, columns, types, rows):
	"""Write a real Ignition DataSet to a tag.

	The testing/tags endpoint writes raw JSON which doesn't produce a native
	DataSet object.  This helper uses system.dataset.toDataSet() to create
	a proper DataSet, then writes it via system.tag.writeBlocking().

	Args:
		tag_path (str): Full tag path, e.g. "[default]Path/To/Tag".
		columns (list[str]): Column header names.
		types (list[str]): Column type names matching Ignition types
			(String, Float8, Int4, Boolean, DateTime, etc.).
		rows (list[list]): Row data - each inner list matches column order.

	Returns:
		dict: {"success": True/False, "rowCount": int, "error": str or None}
	"""
	try:
		# Coerce row values to match declared types
		def parse_boolean(value):
			# Review round: bool("false") is True in Python -- DataSet
			# test rows must parse Boolean strings explicitly.
			if isinstance(value, bool):
				return value
			text = str(value).strip().lower()
			if text in ("true", "1", "yes", "on"):
				return True
			if text in ("false", "0", "no", "off", ""):
				return False
			return bool(value)

		type_coerce = {
			"String": str,
			"Float8": float,
			"Float4": float,
			"Int4": int,
			"Int8": int,
			"Boolean": parse_boolean,
		}
		coerced_rows = []
		for row in rows:
			new_row = []
			for j, val in enumerate(row):
				t = types[j] if j < len(types) else "String"
				fn = type_coerce.get(t)
				new_row.append(fn(val) if fn and val is not None else val)
			coerced_rows.append(new_row)

		ds = system.dataset.toDataSet(columns, coerced_rows)

		# Try writing the DataSet directly first.
		results = system.tag.writeBlocking([tag_path], [ds])
		success = results[0].isGood()

		if success:
			# Verify the read-back is actually a DataSet (not a toString'd string)
			readback = system.tag.readBlocking([tag_path])[0].value
			if readback is not None and hasattr(readback, 'getRowCount'):
				return {
					"success": True,
					"rowCount": ds.getRowCount(),
					"error": None,
				}

			# Tag is String type - Ignition stored the DataSet's toString().
			# Re-configure the tag as DataSet type and retry.
			base_path = tag_path[:tag_path.rfind('/')]
			tag_name = tag_path[tag_path.rfind('/') + 1:]
			system.tag.configure(base_path, [{
				"name": tag_name,
				"dataType": "DataSet",
				"valueSource": "memory",
			}], "o")

			# Retry write with DataSet type
			results = system.tag.writeBlocking([tag_path], [ds])
			success = results[0].isGood()

		return {
			"success": success,
			"rowCount": ds.getRowCount(),
			"error": None if success else str(results[0]),
		}
	except Exception:
		import traceback
		return {
			"success": False,
			"rowCount": 0,
			"error": traceback.format_exc(),
		}


def clear_dataset_tag(tag_path, columns, types):
	"""Write an empty DataSet to a tag.

	Args:
		tag_path (str): Full tag path.
		columns (list[str]): Column header names.
		types (list[str]): Column type names.

	Returns:
		dict: {"success": True/False, "error": str or None}
	"""
	return write_dataset_tag(tag_path, columns, types, [])

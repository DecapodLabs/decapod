"""Focused same-host independent JSON interpretation, not an authority verifier."""
import json
import math
import sys


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate object key")
        result[key] = value
    return result


def finite_float(text):
    value = float(text)
    if not math.isfinite(value):
        raise ValueError("nonfinite number")
    return value


def reject_constant(_):
    raise ValueError("non-JSON number")


try:
    json.loads(
        sys.argv[1],
        object_pairs_hook=unique_object,
        parse_float=finite_float,
        parse_constant=reject_constant,
    )
except ValueError:
    print("invalid_json")
else:
    print("accepted")

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


def finite_integer(text):
    value = int(text)
    if -(1 << 63) <= value <= (1 << 64) - 1:
        return value
    return finite_float(text)


def validate_scalars_and_depth(value):
    pending = [(value, 0)]
    while pending:
        item, depth = pending.pop()
        if isinstance(item, str):
            if any(0xD800 <= ord(character) <= 0xDFFF for character in item):
                raise ValueError("unpaired Unicode surrogate")
        elif isinstance(item, (dict, list)):
            depth += 1
            if depth > 127:
                raise ValueError("JSON container depth exceeded")
            if isinstance(item, dict):
                pending.extend((key, depth) for key in item)
                pending.extend((child, depth) for child in item.values())
            else:
                pending.extend((child, depth) for child in item)


try:
    raw = sys.stdin.buffer.read().decode("utf-8") if sys.argv[1] == "--stdin" else sys.argv[1]
    value = json.loads(
        raw,
        object_pairs_hook=unique_object,
        parse_float=finite_float,
        parse_int=finite_integer,
        parse_constant=reject_constant,
    )
    validate_scalars_and_depth(value)
except (ValueError, RecursionError):
    print("invalid_json")
else:
    print("accepted")

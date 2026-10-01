"""Small, fail-closed validator for the checked-in test-schema dialect.

Not a general JSON Schema implementation: unsupported keywords are errors.
This keeps the qualification harness stdlib-only without silently ignoring rules.
"""
import re


def validate(value, schema, root=None, path="$", check_schema=True):
    root = schema if root is None else root
    supported = {"$schema", "$id", "$defs", "$ref", "title", "description", "type",
                 "properties", "required", "additionalProperties", "items", "enum",
                 "const", "anyOf", "oneOf", "minItems", "minLength", "minimum", "pattern"}
    if check_schema:
        def walk(node):
            if not isinstance(node, dict):
                raise ValueError("schema node must be an object")
            unknown = set(node) - supported
            if unknown:
                raise ValueError(f"unsupported schema keywords: {sorted(unknown)}")
            for key in ("properties", "$defs"):
                for child in node.get(key, {}).values():
                    walk(child)
            for key in ("items", "additionalProperties"):
                if isinstance(node.get(key), dict):
                    walk(node[key])
            for key in ("anyOf", "oneOf"):
                for child in node.get(key, []):
                    walk(child)
        walk(root)
    if "$ref" in schema:
        reference = schema["$ref"]
        if not reference.startswith("#/"):
            raise ValueError(f"external schema reference forbidden: {reference}")
        target = root
        for part in reference[2:].split("/"):
            target = target[part.replace("~1", "/").replace("~0", "~")]
        validate(value, target, root, path, False)
    types = {"object": lambda x: isinstance(x, dict),
             "array": lambda x: isinstance(x, list),
             "string": lambda x: isinstance(x, str),
             "integer": lambda x: isinstance(x, int) and not isinstance(x, bool),
             "number": lambda x: isinstance(x, (int, float)) and not isinstance(x, bool),
             "boolean": lambda x: isinstance(x, bool), "null": lambda x: x is None}
    if "type" in schema:
        choices = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
        if not any(types[k](value) for k in choices):
            raise ValueError(f"{path}: expected {choices}")
    for keyword in ("anyOf", "oneOf"):
        if keyword not in schema:
            continue
        matches = 0
        for choice in schema[keyword]:
            try:
                validate(value, choice, root, path, False)
                matches += 1
            except ValueError:
                pass
        if matches == 0 or (keyword == "oneOf" and matches != 1):
            raise ValueError(f"{path}: {keyword} matched {matches} alternatives")
    if "enum" in schema and value not in schema["enum"]:
        raise ValueError(f"{path}: outside enum")
    if "const" in schema and value != schema["const"]:
        raise ValueError(f"{path}: incorrect constant")
    if isinstance(value, dict):
        missing = set(schema.get("required", [])) - set(value)
        if missing:
            raise ValueError(f"{path}: missing {sorted(missing)}")
        properties = schema.get("properties", {})
        for key, item in value.items():
            child = properties.get(key, schema.get("additionalProperties", {}))
            if child is False:
                raise ValueError(f"{path}: unknown field {key}")
            if isinstance(child, dict):
                validate(item, child, root, f"{path}.{key}", False)
    if isinstance(value, list):
        if len(value) < schema.get("minItems", 0):
            raise ValueError(f"{path}: too few items")
        if "items" in schema:
            for index, item in enumerate(value):
                validate(item, schema["items"], root, f"{path}[{index}]", False)
    if isinstance(value, str):
        if len(value) < schema.get("minLength", 0):
            raise ValueError(f"{path}: too short")
        if "pattern" in schema and re.search(schema["pattern"], value) is None:
            raise ValueError(f"{path}: pattern mismatch")
    if "minimum" in schema and isinstance(value, (int, float)) and value < schema["minimum"]:
        raise ValueError(f"{path}: below minimum")

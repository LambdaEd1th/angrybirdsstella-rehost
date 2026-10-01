#!/usr/bin/env python3
"""Read original NSKeyedArchiver/NIBArchive nibs; emit resolved UI objects read-only."""

import argparse
import json
import plistlib
import struct
from pathlib import Path


def read_archive(path: Path):
    data = path.read_bytes()
    if not data.startswith(b"NIBArchive"):
        return plistlib.loads(data)
    # Original iOS 8 nib variant: four offset/count tables after version words.
    (_, _, object_count, object_offset, key_count, key_offset,
     value_count, value_offset, class_count, class_offset) = struct.unpack_from("<10I", data, 10)
    position = 0

    def number():
        nonlocal position
        result = shift = 0
        while True:
            byte = data[position]
            position += 1
            result |= (byte & 0x7f) << shift
            if byte & 0x80:
                return result
            shift += 7

    def take(count):
        nonlocal position
        result = data[position:position + count]
        if len(result) != count:
            raise ValueError("Truncated NIBArchive")
        position += count
        return result

    position = object_offset
    descriptors = [(number(), number(), number()) for _ in range(object_count)]
    position = key_offset
    keys = [take(number()).decode("utf-8") for _ in range(key_count)]
    position = class_offset
    classes = []
    for _ in range(class_count):
        length = number()
        take(number() * 4)
        classes.append(take(length).rstrip(b"\0").decode("utf-8"))
    position = value_offset
    values = []
    for _ in range(value_count):
        key = keys[number()]
        kind = take(1)[0]
        if kind in (0, 1, 2, 3):
            value = int.from_bytes(take(1 << kind), "little")
        elif kind in (4, 5):
            value = kind == 5
        elif kind in (6, 7):
            value = struct.unpack("<f" if kind == 6 else "<d", take(4 if kind == 6 else 8))[0]
        elif kind == 8:
            value = take(number())
        elif kind == 9:
            value = None
        elif kind == 10:
            value = plistlib.UID(int.from_bytes(take(4), "little"))
        else:
            raise ValueError(f"Unknown NIBArchive value type {kind}")
        values.append((key, value))
    objects = []
    for class_index, start, count in descriptors:
        item = {"$class": plistlib.UID(object_count + class_index)}
        for key, value in values[start:start + count]:
            if key in ("NS.objects", "UINibEncoderEmptyKey"):
                item.setdefault(key, []).append(value)
            else:
                item[key] = value
        inlined = item.pop("UINibEncoderEmptyKey", None)
        if inlined is not None:
            if classes[class_index] in ("NSMutableArray", "NSArray"):
                item["NS.objects"] = inlined
            elif classes[class_index] in ("NSMutableDictionary", "NSDictionary"):
                item["NS.keys"] = inlined[::2]
                item["NS.objects"] = inlined[1::2]
        objects.append(item)
    objects.extend({"$classname": name} for name in classes)
    return {"$objects": objects}


def extract(path: Path):
    archive = read_archive(path)
    objects = archive["$objects"]

    def resolve(value, seen=()):
        if isinstance(value, plistlib.UID):
            index = value.data
            if index in seen:
                return {"reference": index}
            return resolve(objects[index], (*seen, index))
        if isinstance(value, dict):
            if "NS.bytes" in value:
                return value["NS.bytes"].decode("utf-8")
            if "NS.intval" in value:
                return value["NS.intval"]
            result = {}
            for key, item in value.items():
                if key in ("UISuperview", "UISubviews", "UINibEncoderEmptyKey"):
                    continue
                if key in ("UISource", "UIDestination"):
                    result[key] = item.data
                    continue
                if key == "$class":
                    result["class"] = objects[item.data].get("$classname")
                else:
                    result[key] = resolve(item, seen)
            return result
        if isinstance(value, list):
            return [resolve(item, seen) for item in value]
        if isinstance(value, bytes):
            if value[:1] in (b"\x06", b"\x07"):
                width = 4 if value[0] == 6 else 8
                if (len(value) - 1) % width == 0:
                    return list(struct.unpack("<" + ("f" if width == 4 else "d") * ((len(value) - 1) // width), value[1:]))
            return {"hex": value.hex()}
        return value

    names = {}
    parents = {}
    for index, item in enumerate(objects):
        if not isinstance(item, dict):
            continue
        class_info = objects[item["$class"].data] if "$class" in item else {}
        if class_info.get("$classname") == "UIRuntimeOutletConnection":
            names[item["UIDestination"].data] = resolve(item["UILabel"])
        if "UISubviews" in item:
            for child in objects[item["UISubviews"].data].get("NS.objects", []):
                parents[child.data] = index
    ordered = []

    def visit(index):
        ordered.append(index)
        item = objects[index]
        if "UISubviews" in item:
            for child in objects[item["UISubviews"].data].get("NS.objects", []):
                visit(child.data)

    for index, item in enumerate(objects):
        if isinstance(item, dict) and "UIBounds" in item and index not in parents:
            visit(index)
    ordered.extend(index for index, item in enumerate(objects)
                   if isinstance(item, dict) and "UILabel" in item and index not in ordered)
    result = []
    for index in ordered:
        item = objects[index]
        if not isinstance(item, dict):
            continue
        if "UIBounds" not in item and "UILabel" not in item:
            continue
        # Child views are listed separately, keeping the output small and stable.
        selected = {key: value for key, value in item.items() if key != "UISubviews"}
        result.append({"object": index, "name": names.get(index), "parent": parents.get(index), **resolve(selected, (index,))})
    return {"source": str(path), "objects": result}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("nib", type=Path, nargs="+")
    parser.add_argument("--summary", action="store_true")
    arguments = parser.parse_args()
    for path in arguments.nib:
        data = extract(path)
        if arguments.summary:
            print(path)
            keys = ("object", "name", "parent", "class", "UIBounds", "UICenter", "UIHidden", "UITag", "UIImage", "UIText", "UIPlaceholder", "UITextFieldBackground", "UIButtonStatefulContent", "UIFont", "UITextColor", "UITextAlignment")
            for item in data["objects"]:
                if "UIBounds" in item:
                    print(json.dumps({key: item[key] for key in keys if key in item}, ensure_ascii=False))
        else:
            print(json.dumps(data, ensure_ascii=False, indent=2))

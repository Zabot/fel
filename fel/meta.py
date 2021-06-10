known_metadata = {
    "fel-stack": None,
    "fel-stack-index": int,
    "fel-pr": int,
    "fel-branch": None,
    "fel-amended-from": None,
    "fel-version": None,
}


def parse_meta(message):
    sections = message.split("\n---\n")
    if len(sections) == 1:
        return message, {}

    assert len(sections) == 2

    meta_lines = sections[1].strip().split("\n")
    # print([kv.split(': ') for kv in meta_lines])
    metadata = dict([kv.split(": ") for kv in meta_lines])

    for key in metadata:
        try:
            parser = known_metadata[key]
            if parser is not None:
                metadata[key] = parser(metadata[key])
        except KeyError:
            raise KeyError("Unknown metadata key: %s", key)

    return sections[0], metadata


def dump_meta(message, meta):
    message = [message, "---"]
    for key, value in meta.items():
        message.append("{}: {}".format(key, value))

    return "\n".join(message)


def meta(commit, key):
    _, meta = parse_meta(commit.message)
    return meta[key]

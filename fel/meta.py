def parse_meta(message):
    sections = message.split("\n---\n")
    if len(sections) == 1:
        return message, {}

    assert len(sections) == 2

    meta_lines = sections[1].strip().split('\n')
    metadata = dict([kv.split(': ') for kv in meta_lines])

    metadata['fel-pr'] = int(metadata['fel-pr'])

    return sections[0], metadata


def dump_meta(message, meta):

    message = [message, '\n---']
    for key, value in meta.items():
        message.append("{}: {}".format(key, value))

    return '\n'.join(message)

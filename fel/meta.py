# class FelMeta:

def parse_meta(m):
    sections = m.split("\n---\n")
    if len(sections) == 1:
        return m, {}

    assert len(sections) == 2

    meta_lines = sections[1].strip().split('\n')
    metadata = dict([kv.split(': ') for kv in meta_lines])

    metadata['fel-pr'] = int(metadata['fel-pr'])

    return sections[0], metadata


def dump_meta(message, meta):

    message = [message, '\n---']
    for k, v in meta.items():
        message.append("{}: {}".format(k, v))
    
    return '\n'.join(message)

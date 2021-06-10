ok = '\033[32m'
warn = '\033[33m'
fail = '\033[31m'
default = '\033[0m'
info = '\033[34m'
dull = '\033[2;36m'
context = warn #'\033[34m'

def wrap(text, style):
    return "{}{}{}".format(style, text, default)

# ok = '\033[32m ✓'
# warn = '\033[33m •'
# fail = '\033[31m ✖'

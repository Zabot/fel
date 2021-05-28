import yaml

# These configs are populated by default
default_config = {
        'upstream': 'master',
        'check_for_updates': True,
        'gh_token': None,
}

# These fields are required to be non-null
required_fields = ['gh_token']

def load_config(filepath):
    config = default_config

    with open(filepath, "r") as config_yaml:
        loaded_config = yaml.safe_load(config_yaml)
        if loaded_config is not None:
            config.update(loaded_config)

    for field in required_fields:
        if field not in config or config[field] is None:
            raise KeyError("Missing required config field: %s", field)

    return config

# Running locally
Prepare a development environment for fel by with [poetry](https://python-poetry.org/).
Poetry is a packaging and dependency managment tool that installs all of fel's
dependencies in a virtual environment.

```
poetry install
poetry run pytest
poetry run fel --version
```

Any changes you make to the source files will be reflected inside of the virtual
environment without reinstalling. To use fel locally outside of the virtual
environment, install it with `pip`.

```
pip install .
```

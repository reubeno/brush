"""Shared skip-list handling for the Python adapters.

Load as a pytest plugin (`pytest -p e2e_skip`) and every test named in `$E2E_SKIP_LIST` is
skipped, so an adapter needs no skip code of its own. An adapter that must skip before
collection -- because the test would otherwise be generated at all -- calls `parse` directly.
"""

import os

import pytest


def parse(text):
    """The entries in one of the `*-list.txt` files: one per line, `#` starts a comment."""
    return {
        line
        for raw in text.splitlines()
        if (line := raw.strip()) and not line.startswith("#")
    }


@pytest.fixture(autouse=True)
def _skip_listed(request):
    if request.node.name in parse(os.environ.get("E2E_SKIP_LIST", "")):
        pytest.skip("listed in skip-list.txt")

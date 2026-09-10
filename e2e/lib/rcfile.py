"""The rcfile every adapter's interactive shell starts from.

Each suite needs the same two things before the app's own integration lines: a prompt it can
recognize on screen, and no history file, so tests neither read nor pollute the real one.
"""


def write_rc(path, prompt, *lines):
    """Writes `path` and returns it. `prompt` is the PS1 text, without its trailing space."""
    path.write_text("".join([f'PS1="{prompt} "\n', "HISTFILE=\n", *(f"{line}\n" for line in lines)]))
    return path

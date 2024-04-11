#!/bin/env python3
import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument(
    "--rebuild-container-image",
    help="Rebuild the devcontainer image and recreate the container before entering",
    dest="rebuild_container_image",
    action="store_true",
)
parser.add_argument(
    "--restart-container",
    help="Restart the devcontainer before entering",
    dest="restart_container",
    action="store_true",
)
parser.add_argument(
    "--command",
    help="Command to run",
    dest="command",
    default="bash",
)
parser.add_argument(
    "--quiet",
    help="Only generate minimal output",
    dest="quiet",
    action="store_true",
)
parser.add_argument(
    "--env-file",
    help="Environment file to pass into the container",
    dest="env_file",
    type=str,
)

args = parser.parse_args()

if args.rebuild_container_image:
    # One implies the other.
    args.restart_container = True

this_script_dir = os.path.dirname(os.path.abspath(__file__))
source_root = os.path.join(this_script_dir, "..")

if not os.path.isdir(os.path.join(source_root, ".devcontainer")):
    sys.stderr.write("error: couldn't find repo root\n")
    sys.exit(1)

if not shutil.which("devcontainer"):
    sys.stderr.write(
        "error: couldn't find devcontainer CLI; did you install it as a prerequisite?\n"
    )
    sys.exit(1)

devcontainer_cli_args = [
    "devcontainer",
    "up",
    "--workspace-folder",
    source_root,
]

if args.rebuild_container_image or args.restart_container:
    restart_args = devcontainer_cli_args.copy()

    if args.rebuild_container_image:
        restart_args.append("--build-no-cache")
    if args.restart_container:
        restart_args.append("--remove-existing-container")

    if args.rebuild_container_image:
        sys.stderr.write("Rebuilding devcontainer image...\n")
    else:
        sys.stderr.write("Restarting devcontainer...\n")

    subprocess.run(restart_args, check=True, capture_output=args.quiet)

if not args.quiet:
    sys.stderr.write("Ensuring devcontainer is up...\n")

up_result = subprocess.run(devcontainer_cli_args, check=False, capture_output=True)

if up_result.returncode != 0:
    sys.stderr.write("error: devcontainer start-up failed\n")
    sys.stderr.write(up_result.stderr.decode("utf-8"))
    sys.exit(1)

result_data = json.loads(up_result.stdout.decode("utf-8"))

if "outcome" not in result_data:
    sys.stderr.write("error: couldn't determine outcome of devcontainer start-up\n")
    sys.exit(1)

if result_data["outcome"] != "success":
    sys.stderr.write("error: devcontainer start-up failed\n")
    sys.exit(1)

if "containerId" not in result_data:
    sys.stderr.write("error: couldn't determine container ID\n")
    sys.exit(1)

if "remoteWorkspaceFolder" not in result_data:
    sys.stderr.write("error: couldn't determine remote workspace folder\n")
    sys.exit(1)

container_id = result_data["containerId"]
remote_workspace_folder = result_data["remoteWorkspaceFolder"]

if not args.quiet:
    sys.stderr.write("Launching shell in devcontainer...\n")

exec_args = [
    "docker",
    "container",
    "exec",
    "-it",
    "--workdir",
    remote_workspace_folder,
]

if args.env_file:
    exec_args += ["--env-file", args.env_file]

exec_args.append(container_id)

exec_args += shlex.split(args.command)

exec_result = subprocess.run(exec_args, check=False)

sys.stderr.write("Command exited; devcontainer will remain running.\n")

sys.exit(exec_result.returncode)

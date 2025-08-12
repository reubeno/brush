#!/usr/bin/env python3
import argparse
import os
import shutil
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument("-b", "--bash-source", dest="bash_sources_dir", required=True)
parser.add_argument("-s", "--shell", dest="shell", required=False)
parser.add_argument("-v", "--verbose", dest="verbose", action="store_true")

subparsers = parser.add_subparsers(help="Test command", dest="command", required=True)

suite_parser = subparsers.add_parser("suite", help="Run test suite")
suite_parser.add_argument("suite_name", choices=["minimal", "all"], default="minimal")

test_parser = subparsers.add_parser("test", help="Run test case")
test_parser.add_argument("--raw", dest="raw", action="store_true", help="Run test in raw mode without diffing")
test_parser.add_argument("test_name")

diff_parser = subparsers.add_parser("diff", help="Diff test output with an oracle")
diff_parser.add_argument("--oracle", dest="oracle", required=True, help="Path to oracle shell")
diff_parser.add_argument("test_name")

list_parser = subparsers.add_parser("list", help="List test cases")

args = parser.parse_args()

if not args.shell:
    args.shell = os.path.join(args.bash_sources_dir, "bash")

# Make sure we resolve a full path to the shell.
args.shell = shutil.which(args.shell)
if not args.shell:
    sys.stderr.write("failed to resolve specified shell")
    sys.exit(1)

bash_tests_dir = os.path.join(args.bash_sources_dir, "tests")

env = os.environ.copy()
env["BUILD_DIR"] = args.bash_sources_dir
env["THIS_SH"] = args.shell
env["PATH"] = bash_tests_dir + ":" + env["PATH"]

if args.command == "suite":
    cmd = ["sh"]

    if args.verbose:
        cmd.append("-x")

    cmd += ["run-" + args.suite_name]

    subprocess.run(
        cmd,
        cwd=bash_tests_dir,
        env=env,
        check=True
    ) 

elif args.command == "test" and args.raw:
    cmd = [args.shell]

    if args.verbose:
        cmd.append("-x")
        env["THIS_SH"] += " -x"

    cmd += [args.test_name + ".tests"]

    subprocess.run(
        cmd,
        cwd=bash_tests_dir,
        env=env,
        check=True
    )

elif args.command == "test":
    with tempfile.TemporaryDirectory() as temp_dir:
        test_output_path = os.path.join(temp_dir, "test.out") 
        env["BASH_TSTOUT"] = test_output_path
       
        cmd = ["bash"]

        if args.verbose:
            cmd.append("-x")

        cmd += ["-c", f"shopt -s expand_aliases; alias diff='diff --color=always -u'; source {'run-' + args.test_name}"]

        subprocess.run(
            cmd,
            cwd=bash_tests_dir,
            env=env,
            check=True
        )

elif args.command == "diff":
    test_script_name = args.test_name + ".tests"

    with tempfile.TemporaryDirectory() as temp_dir:
        test_cmd = [args.shell]

        if args.verbose:
            test_cmd.append("-x")
            env["THIS_SH"] += " -x"

        out_path = os.path.join(temp_dir, "test.out")

        test_cmd += ["-c", f"source {test_script_name} >{out_path} 2>&1"]

        subprocess.run(test_cmd, cwd=bash_tests_dir, env=env, check=False)

        # ---------------------

        env["THIS_SH"] = args.oracle

        oracle_cmd = [args.oracle]

        if args.verbose:
            oracle_cmd.append("-x")
            env["THIS_SH"] += " -x"

        oracle_out_path = os.path.join(temp_dir, "oracle.out")

        oracle_cmd += ["-c", f"source {test_script_name} >{oracle_out_path} 2>&1"]

        subprocess.run(oracle_cmd, cwd=bash_tests_dir, env=env, check=False)

        # -----------------------

        subprocess.run(["diff", "-u", "--color=always", oracle_out_path, out_path], check=False)

elif args.command == "list":
    test_names = set()
    for filename in os.listdir(bash_tests_dir):
        if filename.endswith(".tests"):
            test_names.add(os.path.splitext(filename)[0])

    for test_name in sorted(test_names):
        print(test_name)

else:
    sys.stderr.write("invalid command")
    sys.exit(1)

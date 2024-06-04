# Integration testing

Our approach to integration testing relies heavily on using test oracles to provide the "correct" answers/expectations for test cases. In practice, we use existing alternate shell implementations as oracles. 

Test cases are defined in YAML files. The test cases defined in a given file comprise a test case set. Running the integration tests for this project executes test case sets in parallel.

```yaml
name: "Example tests"
cases:
  - name: "Basic usage"
    stdin: |
      echo hi
```

This defines a new test case set with the name "Example tests". It contains one defined test case called "Basic usage". This test case will launch the shell without any additional custom arguments (beyond a few standard ones to disable processing default profiles and rc files), write "echo hi" (with a trailing newline) to stdin of the shell, and then close that stream. The test harness will capture the shell's stdout, stderr, and exit code. After repeating these steps with the test oracle, each of these 3 data are compared. An error is flagged if any of the 3 differ. 

Test cases are run with the working directory initialized to a temporary directory. The contents of the temporary directory are inspected after the shell-under-test has exited, and compared against their counterparts in the oracle's run. This enables easy checking of files created, deleted, or mutated as side effects of running the test case. 
name: "@RES@ (){ ;: }"
cases:
  - name: "@RES@ (){ ;: }"
    stdin: |
      @RES@ (){ :; }; echo "@RES@ (){ :; }" => $?"

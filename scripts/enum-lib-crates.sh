#!/bin/bash
set -euo pipefail

cargo metadata --no-deps --format-version 1 | jq -r '
  .workspace_members as $members
  | .packages[]
  | select(.id as $id | $members | index($id))
  | select(any(.targets[]?; any(.kind[]?; . == "lib")))
  | .name
'

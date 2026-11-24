# How to release

_(This is only relevant for project maintainers.)_

* Install [release-plz](https://github.com/MarcoIeni/release-plz)
* Checkout the `main` branch (with a clean working tree).
* Refresh the compatibility-test count badge in `README.md` if it has drifted (round down to the nearest hundred; count `- name:` entries under `brush-shell/tests/cases/compat`).
* Run: `release-plz update`. Review its changes, notably including the changelog updates.
* Review each crate against its own published baseline. Add required API migration guidance and [finalize the guides manually](../reference/migrations.md#finalizing-a-guide-for-release) after reviewing the selected versions. Release-plz does not update guide paths or rustdoc wiring.
* Run the [guide checks](../reference/migrations.md#validating-guides), semver checks against published library versions, candidate package verification and `cargo xtask ci full`. Report incomplete review and failed checks rather than treating the release as ready.
* PR through any generated changes with a `chore: prepare release` commit summary.
* After the changes have merged into `main`, update your local `main` branch.
* Acquire GitHub and `crates.io` tokens that have sufficient permissions to publish.
* Authenticate with `crates.io` by running: `cargo login`.
* Run: `release-plz release --backend github --git-token <TOKEN>`.
* Verify the intended crate versions and tags were published, then review the GitHub release draft and its migration links.
* When publication is authorized, publish the verified draft with `gh release edit "$tag" --draft=false --latest` (set `tag` from the verified release identity). The existing CD workflow builds and uploads assets after publication; it does not attach final assets while the release remains a draft.
* Run: `cargo install --locked brush-shell` to verify the release.

# How to release

_(This is only relevant for project maintainers.)_

* Install [release-plz](https://github.com/MarcoIeni/release-plz)
* Checkout the `main` branch (with a clean working tree).
* Run: `release-plz update`. Review its changes, notable including the changelog updates.
* PR through any generated changes with a `chore: prepare release` commit summary.
* After the changes have merged into `main`, update your local `main` branch.
* Acquire GitHub and `crates.io` tokens that have sufficient permissions to publish.
* Authenticate with `crates.io` by running: `cargo login`.
* Run: `release-plz release --backend github --git-token <TOKEN>`.
* Update the published GitHub release to include an auto-generated changelog.
* Run: `cargo install --locked brush-shell` to verify the release.

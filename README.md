# cli

The Zoo command line tool.

The instructions below refer to instructions for contributing to the repo.

For the CLI docs for end users refer to: https://zoo.dev/docs/cli/manual

### Installing

On MacOS, you can use Homebrew:
```
brew tap kittycad/kittycad
brew install kittycad
zoo --help   
```
For all other installs, see the instructions on the [latest release](https://github.com/KittyCAD/cli/releases).

### Nix

From a checkout of this repository:

```sh
nix build .#zoo
nix run .#zoo -- --help
```

The flake exports `packages.<system>.zoo` for use in NixOS or Home Manager
configurations. `nix develop` provides the development environment.

### Updating the API spec

Updating the API spec is as simple as updating the [`spec.json`](spec.json) file. The macro will take it from there when
you `cargo build`. It likely might need some tender love and care to make it a nice command like the other generated ones
if it is out of the ordinary.

Only `create`, `edit`, `view/get`, `list`, `delete` commands are generated. The rest are bespoke and any generation lead to something
that seemed harder to maintain over time. But if you are brave you can try.

For examples of the macro formatting, checkout some of the commands under `src/` like `cmd_file` or `cmd_user`.

**Note:** If you update the API spec here, you will likely want to bump the spec for the [kittycad.rs](https://github.com/KittyCAD/kittycad.rs)
repo as well since that is where the API client comes from.

### Running the tests

The tests use the `ZOO_TEST_TOKEN`  variables for knowing how to authenticate.

### Releasing a new version

1. Make sure the `Cargo.toml` has the new version you want to release.
   When changing `Cargo.toml`, make sure to run `cargo check` to update
   `Cargo.lock`.
    ```
    cargo check
    ```
1. Make sure you have [toml-cli](https://github.com/gnprice/toml-cli) installed.
   ```
   cargo install toml-cli
   ```
2. Run `make tag` this is just an easy command for making a tag formatted
   correctly with the version.
3. Push the tag (the result of `make tag` gives instructions for this)
4. Everything else is triggered from the tag push. Just make sure all the tests
   and cross compilation pass on the `main` branch before making and pushing
   a new tag.

### Building

To build, simply run `cargo build` like usual.

Make sure to update to the latest stable rustc: for example, if you use `rustup`, run `rustup update`.

#### Cross compiling

If you're on Debian or Ubuntu, install the required dependencies by running `.github/workflows/cross-deps.sh`. Otherwise, look there to see what packages are required.

Then, simply run `make`. Binaries will be available in `cross/`.

If you want to only build one of the cross targets, supply the `CROSS_TARGETS` environment variable:

    CROSS_TARGETS=x86_64-unknown-linux-musl make


If you get an error about md5sum on mac when running `make release`, you probably need to `brew install coreutils`

### Offline KCL checks

```bash
zoo kcl check main.kcl
zoo kcl check . --deny-warnings
zoo kcl check main.kcl --json
cat main.kcl | zoo kcl check - --json
```

The default entry point is `main.kcl`; directories use their `main.kcl`. Checks
use `kcl-lib` mock execution without authentication or an engine connection. The
checker also skips the CLI update notifier, including when invoked through an
alias. A successful check does not establish authoritative CAD geometry.

File inputs use the nearest `project.toml` in the entry point's directory or its
parents. Imports resolve from the entry point's directory. Stdin imports resolve
from the working directory; stdin uses default executor settings and the source's
KCL annotations. Errors fail the command; warnings fail only with
`--deny-warnings`. Text diagnostics go to stderr.

`--json` writes one report to stdout even when checking fails, with
`schema_version: 1`, `ok`, and `diagnostics`. Each diagnostic has a severity,
optional code, message, locations, and optional suggested edit. Locations include
file names, zero-based UTF-8 byte offsets (end exclusive), and one-based line and
Unicode character columns. Unavailable locations are null; stdin is named `-`.
`ok` includes the warning policy and agrees with the exit status. This format
contains diagnostics, not internal execution or geometry data.

The Cargo patches pin our `rockboynton/modeling-app` fork containing the shared
`kcl-lib::check` API. No sibling checkout is required. Replace the Git patches
with released crate dependencies once the shared API is available upstream.

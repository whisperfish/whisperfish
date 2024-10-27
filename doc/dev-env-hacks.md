# Cool hacks for development

These represent the views of the authors and are not officially
supported by the Whisperfish project. Corrections, additions and
suggestions welcome!

## Rust protips

If you want to build locally against local changes to
[service](https://github.com/Michael-F-Bryan/libsignal-service-rs) or
[protocol](https://github.com/Michael-F-Bryan/libsignal-protocol-rs),
you should edit `Cargo.toml` where applicable:

    [patch."https://github.com/Michael-F-Bryan/libsignal-service-rs"]
    libsignal-service = { path = "/PATH/TO/GIT/CLONE/libsignal-service-rs/libsignal-service" }
    libsignal-service-hyper = { path = "PATH/TO/GIT/CLONE/libsignal-service-rs/libsignal-service-hyper" }

    [patch."https://github.com/Michael-F-Bryan/libsignal-protocol-rs"]
    libsignal-protocol = { path = "/PATH/TO/GIT/CLONE/libsignal-protocol-rs/libsignal-protocol" }

## ZSH

Using [autoenv](https://github.com/zpm-zsh/autoenv) you can activate the
environment by symlinking it:

    ln -s .env .in

Then you can have an `.out` file as well, which is useful for cleaning
the mess from your environment when leaving the directory, or sourcing
it manually to run `cargo test` outside the ARM-cross-compilation
situation.

Furthermore if you have Python's virtualenv support in your prompt, you
can set its environment variable. A bit of a hacky overload but having
these things visible in the prompt is useful.

    unset RUST_SRC_PATH
    unset RUST_BACKTRACE
    unset MERSDK
    unset MER_TARGET
    unset RUSTFLAGS
    unset SSH_TARGET

    test ! -z $VIRTUAL_ENV && unset VIRTUAL_ENV

## (Neo)Vim

In the Vim development process, the code is represented by two separate
yet equally important plugins: The autocompleter, which helps with crate
contents, and the linter, which checks for your mistakes. These are
their stories. **DUN DUN**.

### Deoplete

[Deoplete](https://github.com/Shougo/deoplete.nvim) is the preferred
completion framework, as it allows for different sources to be used.

[Deoplete-Rust](https://github.com/sebastianmarkow/deoplete-rust) is the
source plugin of choice. It uses
[Racer](https://github.com/racer-rust/racer) to do the heavy lifting.

Deoplete-Rust respects the `RUST_SRC_PATH` variable, so all
you have to configure is

    let g:deoplete#sources#rust#racer_binary='/path/to/racer'

Note that Racer must be installed from Nightly but it professes to know
all the channels. Have the stable Rust source available:

    rustup component add rust-src --toolchain stable

### ALE

[ALE](https://github.com/dense-analysis/ale) is the preferred linter, at
least for now. It also provides a plugin system, which is suitable for
our needs.

It takes its cues from
[rust-analyzer](https://rust-analyzer.github.io/manual.html#rust-analyzer-language-server-binary).

Then configure `g:ale_linters` to include `analyzer`

    let g:ale_linters = {
        'rust': ['analyzer']
    }

Note that you may get a ton of `rustc` processes from this
approach.

    let g:ale_lint_on_save=0

should prevent that from happening. You may see a ton of compilation
happen when starting to edit and running the first `cargo test`,
but after that it should cool down.

## Sailfish IDE (QtCreator)

You can setup the Sailfish IDE for working on the UI part. Create
`harbour-whisperfish.pro` file and open it as new project in QtCreator:

    TARGET = harbour-whisperfish
    CONFIG += sailfishapp_qml

You will be asked to configure "kits". Select only the ARM one and
deselect all build configs except for "debug". (It doesn't matter
which, just keep only one.) Select "project" in the sidebar, choose
"build" and click the tiny crossed circles for all build steps to
disable them. This makes sure you don't accidentally start the build
engine (which you don't need).

All QML and JS files will be picked up automatically. Rust files etc.
won't show up, but there are better tools for that anyways.

Then select the "execution" configuration. Disable the "Rsync:
Deploys with rsync." step but keep the "Prepare Target" step. Click
"add" below "execution" and choose "custom executable". Enter in
the "executable" field:

    path/to/whisperfish/live-patch.sh

Set `-w -B` as command line arguments. This enables watching
for changes (deploying and restarting as needed) and disables
automatically rebuilding the app. (Use `-b` to enable building.)

Now you can click on "run" (or press Ctrl+R) to start the live runner.
All log output will be in the "program output" pane (Alt+3). QML
errors will become clickable links to the respective files.

## Vistual Studio Code

Since Qt Creator doesn't really support Rust, you can use Visual
Studio Code for writing Rust code instead. To make it better handle
the Silica QML (and C++ content), you can copy the include directory
from the build target to your project directory for easier access:

    cp -r ~/SailfishOS/mersdk/targets/SailfishOS-4.5.0.18-aarch64.default/usr/include ~/SFOS/

Then you can add this to your `.vscode/c_cpp_properties.json`:

    {
      "configurations": [
        {
          "name": "Sailfish OS",
          "includePath": [
            "${workspaceFolder}/**",
            "${workspaceFolder}/../include/**"
          ],
          "defines": [],
          "compilerPath": "/usr/bin/clang",
          "cStandard": "c11",
          "cppStandard": "c++11",
          "intelliSenseMode": "linux-clang-x64"
        }
      ],
      (...)
    }

### Workspace with libsignal-service

Since developing Whisperfish frequently requires developing `libsignal-service`,
it's convenient to use a shared workspace so you don't have to keep two Visual
Studio Code windows open.

Open workspace settings JSON and put the project settings in:

```json
{
	"folders": [
		{
			"path": "/home/user/src/whisperfish"
		},
		{
			"path": "/home/uesr/src/libsignal-service-rs"
		}
	],
	"settings": {
		"rust-analyzer.linkedProjects": [
			"/home/user/code/libsignal-service-rs/Cargo.toml"
		]
	}
}
```

Now go ahead and `F2` and `F12` your way across the projects!

## Keeping CI and Clippy happy, usually

The CI in Whisperfish makes sure the Rust code is properly formatted
and idiomatic. This can lead to the following scenario:

- You make a branch
- You make changes and commit them
- You push the branch
- You make a merge commit
- The CI runs
- Clippy has issues with your code
- You mumble and fix your code

To prevent this from happening and to save both your ~~nerves~~ time
and CI from spinning up the pipeline only to get stuck on something
(non-)trivial, you can use a pre-push hook. With Rust 1.75.0 I use this:

    $ cat .git/hooks/pre-push
    #!/bin/sh

    fmt() {
        if [ $2 -eq 0 ]; then
        echo -e "$1:\tOK"
        else
        echo -e "$1:\tFAILED"
        fi
    }

    SRC=$(git rev-parse --show-toplevel)

    grepper() {
        echo -e "$1:\t$(rg "/.*$1" "$2" | wc -l)"
    }

    echo -e "-----\nRunning tests...\n-----\n"
    cargo test --color never -- --color never
    E_TEST=$?

    echo -e "-----\nRunning format...\n-----\n"
    cargo fmt --check -- --color never
    E_FMT=$?

    echo -e "-----\nRunning clippy...\n-----\n"
    cargo clippy --color never --no-deps --all-targets -- -D warnings -A clippy::useless_transmute -A clippy::too-many-arguments
    E_CLIPPY=$?

    echo -e "-----\nRunning qmllint...\n-----\n"
    find qml/ -name "*.qml" -print0 | xargs -0 qmllint
    E_QML=$?

    let "RETVAL = $E_TEST + $E_FMT + $E_CLIPPY + $E_QML"

    echo ""
    grepper FIXME $SRC
    grepper TODO $SRC
    grepper XXX $SRC
    echo ""
    fmt QML $E_QML
    fmt Tests $E_TEST
    fmt Format $E_FMT
    fmt Clippy $E_CLIPPY
    echo ""

    exit $RETVAL

Note that the script requires qmllint and ripgrep. Modify to taste!

For convenience, I also have a separate script to make
`cargo clippy` and `cargo fmt` fix the found issues:

   $ cat fix.sh
    #!/bin/sh
    cargo fmt && cargo clippy --fix --allow-dirty --allow-staged --all-targets -- -D warnings -A clippy::useless-transmute -A clippy::too-many-arguments

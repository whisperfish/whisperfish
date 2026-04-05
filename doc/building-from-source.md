# Building from source

Both Sailfish SDK and Platform SDK are supported. These instructions are for Sailfish SDK but they are easily adaptable to Platform SDK as well.
It is recommened to use Sailfish SDK 3.12.5 or newer. Older versions may work but are not supported.

**Note:** Only the Sailfish SDK Docker build engine supports Rust compiling. VirtualBox build engine will not work.

We often lead Jolla/SailfishOS a bit in Rust version, which means we have to use a self-built Rust. If it wasn't the case,
you could just clone the repo and run `sfdk build` and disregard the rest of the document :)

Ruben provides some more recent LLVM/clang/Rust on a repo on his home server. The instructions below show how to use them.

For more advanced stuff, see [Cool hacks for development](tips-and-tricks.md).

## Prepare the tooling

First, add the repository and install the necessary packages:

    $ cd ~/SFOS/whisperfish
    $ sfdk tools exec SailfishOS-5.0.0.62 bash -c \
        "zypper ar --no-gpgcheck https://nas.rubdos.be/~rsmet/sailfish-repo/ rubdos && zypper ref"
    $ sfdk tools exec SailfishOS-5.0.0.62 bash -c \
        "zypper in --allow-vendor-change rust cargo rust-std-static-aarch64-unknown-linux-gnu rust-std-static-armv7-unknown-linux-gnueabihf rust-std-static-i686-unknown-linux-gnu"

## Prepare the build targets

Then, let's add the repository for build targets. Note that we don't need to manually install anything:

    $ for ARCH in i486 aarch64 armv7hl
        do
        sfdk config --push target SailfishOS-5.0.0.62-$ARCH; sfdk build-shell --maintain bash -c \
            "zypper ar --no-gpgcheck https://nas.rubdos.be/~rsmet/sailfish-repo/ rubdos && zypper ref"
        done

## Compile Whisperfish

> **Important:** If you compiled Wishperfish before with an older Rust version, you **must** run `cargo clean` to clear the old build cache!
>
> This seems to be best indicated by the following error message:
>
>     error[E0786]: found invalid metadata files for crate `std`

Let's build Whisperfish:

    $ for ARCH in i486 armv7hl aarch64
        do
            sfdk -c target=SailfishOS-5.0.0.62-$ARCH build
        done
    [...]
    The following 35 NEW packages are going to be installed:
      cargo                                    1.89.0+git1-1
      rust                                     1.89.0+git1-1
    [...]

The dependencies are installed in the build target as needed. There's no such mechanism for tooling, that's why we had to do it manually.

To verify the installation, early in the build log `rustc` and `cargo` versions are reported:

    [...]
    + rustc --version
    rustc 1.89.0 (29483883e 2025-08-04) (built from a source tarball)
    + cargo --version
    cargo 1.89.0 (c24e10642 2025-06-23) (built from a source tarball)
    [...]

Now the usual advice applies -- grab yourself a nice, hot cup of coffee while Whisperfish builds!

## Building the share plugin

If you want to build the sharing plugin for Whisperfish, use this command (note the double double dashes):

    sfdk build -- --with shareplugin_v2

For Sailfish 4.2 and older, use `--with shareplugin_v1` instead.

If you get errors (command not found or status 126) at linking stage, make sure that you are not using `~/.cargo/config` to override linkers or compilers.

## Subsequent Whisperfish builds

It's recommened to let the first build complete with the defaults, i.e. single threaded compilation. After the first successful build, you should be quite safe with using a low threaded count for rebuilding just the changed sources. If the build hangs, just hit `Ctrl-C` and try it again - you'll figure out if the threaded build works for your computer or not.

    sfdk build -- --define 'jobs 4'

# Building Whisperfish in Sailfish community OBS

Whisperfish gained the ability to be built on OBS in late 2024, but it was lost again due to newer Rust requirements. You can check out the current development package [here](https://build.sailfishos.org/package/show/home:rubdos:whisperfish/Whisperfish).

Chum and OBS don't let us insert e.g. `--with lto` and such, so that needs to be handled differently. Chum sets `%_chum` and the project Whisperfish is built in (manually) sets `%_obs`, so we have [hardwired](https://gitlab.com/whisperfish/whisperfish/-/merge_requests/657/diffs?commit_id=f8bec68a800769c40669136b7d437300852bfbaa) the presence of either of those into `bcond_with` flags.

To mimic OBS build locally, you can use this command:

    sfdk build -- --define="_obs 1"

At the time of writing, the command above is functionally equivalent with:

    sfdk build -- --with lto --with tools --with vendor

Note that you need to locally generate (or download from the link above) `vendor.tar.xz` and `vendor.toml` files. You can do them locally like so:

```bash
# Download dependency sources to `vendor/`
sfdk build-shell cargo vendor
# Copy the contents shown for `.cargo/config.toml`...
vim rpm/vendor.toml # ...and paste it here
# Compress the sources
tar cJf rpm/vendor.tar.xz vendor/
# Build!
sfdk build -- --with vendor
```

Another thing about the `vendor.*` files: they are currently excluded from the git repository on purpose. There is a CI job to generate them however. 

# Experimental or incomplete features

## Voice and video calls

For voice and video calling, Whisperfish requires the RingRTC library,
including Signal's custom WebRTC implementation.  You can download pre-built artifacts for your architecture of choice with the following command:

    bash fetch-webrtc.sh [aarch64|armv7hl|i486|x86_64]

See <https://www.rubdos.be/2024/09/08/building-ringrtc-for-whisperfish.html> for how to build these artifacts.

To build Whisperfish with support for voice and video calls included, use

    sfdk build -- --with calling

This triggers the `cargo build --feature calling` feature flag, which adds voice and video support.

### Building for the host

Building Whisperfish on your host machine is also possible. This is useful for development and debugging purposes. There are some differences to be aware of.

The RPM automatically selects the `sailfish` feature flag, which will not compile outside of SailfishOS.  This feature flag is *not* enabled by default, so it doesn't sit in the way.

You'll have to manually set the `OUTPUT_DIR` variable, which contains the output of the `webrtc` build.  The `fetch-webrtc.sh` script fetches `libwebrtc.a` pre-built for all four architectures by default, and for the two major versions of OpenSSL (3.x, and 1.1.1).

    bash fetch-webrtc.sh
    OUTPUT_DIR=$PWD/ringrtc/322/x86_64-unknown-linux-gnu/ cargo build --features bundled-sqlcipher

You can swap out `322` for `111` if your system uses OpenSSL 1.1.1.

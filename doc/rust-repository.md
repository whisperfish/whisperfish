# Compiling Whisperfish with a more recent Rust

As of 2024-12-29, (self-compiled) Rust 1.75.0 is available for Sailfish SDK 3.10.4 (Sailfish OS 4.5)! The download links are provided in this readme, until Rust 1.75.0 is officially available from Jolla.

Sometimes, however, we lead Jolla/SailfishOS a bit in Rust version, and then Ruben provides some more recent LLVM/clang/Rust on a repo on his home server.
The below instructions show how to add this repository.

## 1) Install Sailfish SDK

The instructions assume you have a freshly-installed Sailfish SDK 3.10.4 (the Docker variant) ready. All three architectures are supported and the build process should be identical.

My setup has the project folder as `~/SFOS`, your setup likely has something different. This guide also assumes you have Whisperfish already cloned in `~/SFOS/whisperfish`.

## 2) Add repositories

I've prepared repositories for all architectures, which makes the package installation much more convenient. Let's start with adding the repository to the tooling first:

    $ cd ~/SFOS/whisperfish
    $ sfdk tools exec SailfishOS-4.5.0.18 bash -c \
        "zypper ar --no-gpgcheck https://nas.rubdos.be/~rsmet/sailfish-repo/ rubdos && zypper ref"
    $ sfdk tools exec SailfishOS-4.5.0.18 bash -c "zypper in --allow-vendor-change rust cargo rust-std-static-aarch64-unknown-linux-gnu rust-std-static-armv7-unknown-linux-gnueabihf rust-std-static-i686-unknown-linux-gnu"

Next, let's add the repositories to the build targets:

    $ for ARCH in i486 aarch64 armv7hl
        do
        sfdk config --push target SailfishOS-4.5.0.18-$ARCH; sfdk build-shell --maintain bash -c \
            "zypper ar --no-gpgcheck https://nas.rubdos.be/~rsmet/sailfish-repo/ rubdos && zypper ref"
        done

## 3) Compile Whisperfish

> **Important:** If you compiled Wishperfish before with Rust 1.52 or 1.72, you **must** run `cargo clean` to clear the old build cache!
>
> This seems to be best indicated by the following error message:
>
>     error[E0786]: found invalid metadata files for crate `std`

It's time:

    $ for ARCH in i486 armv7hl aarch64
        do
            sfdk -c target=SailfishOS-4.5.0.18-$ARCH build
        done
    [...]
    The following 35 NEW packages are going to be installed:
      cargo                                    1.75.0+git1-1
      llvm-libs                                15.0.7-0
      rust                                     1.75.0+git1-1
      rust-std-static-i686-unknown-linux-gnu   1.75.0+git1-1
    [...]

At this stage, when Rust 1.75 is still new to SFOS, it's recommended to build all three architectures to catch any problems.

You'll note that the dependencies are installed in the build target too, but this alone isn't enough -- that's why we had to manually install them in the tooling. (I hope my terminology is correct...)

The packages download and install. Now, the moment of truth: what `rustc` and `cargo` versions are reported?

    [...]
    + rustc --version
    rustc 1.75.0-nightly (82e1608df 2023-12-21) (built from a source tarball)
    + cargo --version
    cargo 1.75.0-nightly
    [...]

Now the usual advice applies -- grab yourself a nice, hot cup of coffee while Whisperfish builds! If you're adventurous, you can update the `-j 1` parameter of `cargo build` with in the .spec file with your core count, keep an eye on the progress (and CPU usage) and *when it hangs*, hit Ctrl-C and run `sfdk build` again. I've also found that it tends to hang a little less often when I use `taskset 0x55555 cargo build` with `-j 8 \` on my 16c32t-core machine. After Whisperfish is build the first time, and changes are made only to Whisperfish sources, using parallel compilation never seems to crash, however. It saves less time then, but as it's *the* repeated step in development, it does end up saving quite a nice amount of time! The "bad" news is that changing the `-j` flag triggers a full rebuild now, which is quite inconvenient.

After it's done, check out the shiny, new RPM files:

    $ ls ~/SFOS/RPMS/SailfishOS-4.5.0.18-*/harbour-whisperfish*
    /home/matti/SFOS/RPMS/SailfishOS-4.5.0.18-aarch64/harbour-whisperfish-0.6.0-0.aarch64.rpm
    /home/matti/SFOS/RPMS/SailfishOS-4.5.0.18-armv7hl/harbour-whisperfish-0.6.0-0.armv7hl.rpm
    /home/matti/SFOS/RPMS/SailfishOS-4.5.0.18-i486/harbour-whisperfish-0.6.0-0.i486.rpm

My RPM name is shorter, because I use `no-fix-version` -- use it if you will:

    sfdk config --global --push no-fix-version

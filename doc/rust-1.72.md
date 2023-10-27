# Compiling Whisperfish with Rust 1.72

Currently, only i486 compiler is available, but it's enough for updating dependencies (removing all the pinned versions, hopefully). It's also possible to run it in the Sailfish OS emulator, which is i486.

Note: I'm doing this all in the SFDK global context, but nothing shouldn't prevent from using a project scope for the settings.

## 1) Install Sailfish SDK

The instructions assume you have a freshly-installed Sailfish SDK 3.10.4 (the Docker variant) ready. My setup has the project folder as `~/SFOS`, your setup likely has something different. The only build target used is SailfishOS-4.5.0.18-i486 and it's installed by default - as aarch64 and armv7hl targets too.

## 2) Prepare the local package repository

SFDK has the ability to pick up a packages from the host to fulfill depencies. My "repository root folder" is `~/SFOS/RPMS`. First, create the folder and setup the location by running:

    $ sfdk config --global --push output-prefix ~/SFOS/RPMS

Eventually, the location `~/SFOS/RPMS/SailfishOs-4.5.0.18-i486` will be auto-created, and there's where we want to put the files:

    $ ls ~/SFOS/RPMS/SailfishOS-4.5.0.18-i486
    llvm-libs-14.0.6-0.i486.rpm
    cargo-1.72.1+git1-1.i486.rpm
    rust-1.72.1+git1-1.i486.rpm
    rust-std-static-i686-unknown-linux-gnu-1.72.1+git1-1.i486.rpm

## 3) Update the Whisperfish .spec file to use the new Rust (I was lazy so I went the short route):

    BuildRequires: rust >= 1.72
    BuildRequires: rust-std-static >= 1.72
    BuildRequires: cargo >= 1.72

## 4) Set the build target

    $ sfdk config --global --push target SailfishOS-4.5.0.18-i486

## 5) Download the packages into the tooling root

This is the "interesting" part of this process -- we'll have to install the packages manually into the Sailfish SDK tooling. This is due to the relationship betweeh build target, build tooling and scratchbox in general, which is something not clear to me -- but nevertheless, it needs to be done, and it works.

Since the packages can't be simply copied into the tooling filesystem, you have to prepare the files on a HTTP server so you can access them. (We are working out a location to push the files to so others can just download them from there.) Once that's done, enter the tooling and download the files:

    $ sfdk tools exec SailfishOS-4.5.0.18 bash
    # cd ~ ; mkdir RPMS ; cd RPMS
    # URL=http://example.com/path
    # for FILE in \
        llvm-libs-14.0.6-0.i486.rpm \
        cargo-1.72.1+git1-1.i486.rpm \
        rust-1.72.1+git1-1.i486.rpm \
        rust-std-static-i686-unknown-linux-gnu-1.72.1+git1-1.i486.rpm; \
        do
            curl $URL/$FILE --output $FILE
        done

## 6) Install the packages in the tooling

Install the packages with Zypper as usual. There will be questions asked, as we don't update the whole "LLVM stack". Yeah, this gets a bit ugly... (I think even with the full stack updated the core dependencies would still be broken.) When asked, choose option 3 -- breake package dependencies.

    # zypper install ~/RPMS/*.rpm

## 7) Compile Whisperfish

It's time:

    $ sfdk build
    [...]
    The following 22 NEW packages are going to be installed:
      cargo                                    1.72.1+git1-1
      llvm-libs                                14.0.6-0
      rust                                     1.72.1+git1-1
      rust-std-static-i686-unknown-linux-gnu   1.72.1+git1-1
      [...]

You'll note that the dependencies are installed in the build target too, but this alone isn't enough -- that's why we had to manually install them in the tooling. (I hope my terminology is correct...)

The packages download and install. Now, the moment of truth: what `rustc` and `cargo` versions are reported?

    [...]
    + rustc --version
    rustc 1.72.1-nightly (d5c2e9c34 2023-09-13) (built from a source tarball)
    + cargo --version
    cargo 1.72.1-nightly
    [...]

Now the usual advice applies -- grab yourself a nice, hot cup of coffee while Whisperfish builds! If you're adventurous, you can update the `-j 1` parameter of `cargo build` with in the .spec file with your core count, keep an eye on the progress (and CPU usage) and *when it hangs*, hit Ctrl-C and restart-continue the process by running `sfdk build` again. After Whisperfish is build the first time, and changes are made only to Whisperfish sources, using parallel compilation never seems to crash, however. It saves less time then, but as it's the repeated step in application development, it does end up saving quite a nice amount of time!

After it's done, check out the shiny, new RPM:

    $ ls ~/SFOS/RPMS/SailfishOS-4.5.0.18-i486/harbour-whisperfish*
    /home/matti/SFOS/RPMS/SailfishOS-4.5.0.18-i486/harbour-whisperfish-0.6.0-0.i486.rpm

My RPM name is shorter, because I use `no-fix-version` -- use it if you will:

    $ sfdk config --global --push no-fix-version

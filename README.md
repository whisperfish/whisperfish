# Whisperfish

Whisperfish is a native [Signal](https://www.whispersystems.org/) client
for [Sailfish OS](https://sailfishos.org/). The user interface is
heavily based on the jolla-messages application written by Jolla Ltd.

Whisperfish has plenty of features these days and is in a mostly usable state.
Join our development channel on Matrix
([#whisperfish:rubdos.be](https://matrix.to/#/#whisperfish:rubdos.be))
or Libera.Chat (#whisperfish) to get in touch, and check
[our wiki](https://gitlab.com/whisperfish/whisperfish/-/wikis/home) to see
whether Whisperfish would work for you.

To install, you have two options:

- Releases from [OpenRepos](https://openrepos.net/content/rubdos/whisperfish)
- "Nightly" builds from
  [the Gitlab Package Registry](https://gitlab.com/whisperfish/whisperfish/-/packages).

In most cases, there should be no need to install from Git directly.
We push regular updates to OpenRepos, when they make sense.

Please mind that Whisperfish in still in *beta condition*, which means
that certain things do not work, other things make the application
crash, and I've heard reports that beta software can be a cause for
dogs eating homework. You've been warned. On the other hand, we have
many people happily using Whisperfish as daily driver, and we make up
for lacking features in our community support in the aforementioned
Matrix and IRC room. Please come say hello! We don't bite (we may
byte), and we don't eat homework.

## Project Status

This project started from a now outdated Go-based SailfishOS client for
Signal. This version, 0.6 and onwards, is a complete rewrite, and uses
[libsignal-client](https://github.com/signalapp/libsignal-client)
instead. This means we aim for better maintainability. It also means the
whole SailfishOS app had to be rewritten, and you may want to make a
back-up of your current files if you still come from 0.5. Specifically:

- `.local/share/harbour-whisperfish` contains all your data.
- `.config/harbour-whisperfish` contains the apps configuration.

In current releases the paths have changed:

- `.local/share/be.rubdos/harbour-whisperfish`
- `.config/be.rubdos/harbour-whisperfish`

## Features

- [x] Registration
- [ ] Contact Discovery
- [x] Direct messages
- [x] Group messages
- [ ] Editing messages
- [x] Sealed sending
- [x] Storing conversations
- [x] Sending attachments
- [x] Taking a photo as an attachment
- [ ] Taking a video as an attachment
- [x] Encrypted identity and session store
- [x] Encrypted message store
- [x] Advanced user settings
- [x] Multi-Device support (links with Signal Desktop)
- [ ] Encrypted local attachment store
- [x] Archiving conversations
- [x] Muting conversations
- [ ] Voice calls
- [ ] Video calls

Please search the
[issue tracker](https://gitlab.com/whisperfish/whisperfish/-/issues) before
filing any bug report or feature request. Please upvote issues that are
important to you. We use the vote counter for determining a feature's
priority.

## Performance Tips

Whisperfish connects to Signal using Websockets. For a better user
experience try adjusting the power settings on your Jolla to disable
late suspend. This should keep the network interfaces up and allow
Whisperfish to maintain websocket connections even when the device is in
"sleep". This could potentially impact your battery life depending on
your usage. Otherwise every time your device goes into deep sleep, the
Websocket connection is broken and you may not receive messages until
the next time the OS wakes up and Whisperfish reconnects.

To disable late suspend and enable "early suspend" run:

    mcetool --set-suspend-policy=early

See here for more information.

1. <https://together.jolla.com/question/55056/dynamic-pm-in-jolla/>
2. <http://talk.maemo.org/showpost.php?p=1401956&postcount=29>
3. <https://sailfishos.org/wiki/Sailfish_OS_Cheat_Sheet#Blocking_Device_Suspend>

## Building from source

Whisperfish is written in Rust (and QML), and Rust is a bit of a special
entity in Sailfish OS. Luckily, Jolla has provided a more or less decent
Rust compiler since Sailfish OS 3.4 (Rust 1.52), and in Sailfish OS 4.6 we have
a quite recent Rust 1.75, which we can use out-of-the-box. So grab yourself
Sailfish SDK 3.11 or newer (or Platform SDK with said tooling and targets).

**Note:** Only the Docker build engine supports Rust compiling. VirtualBox build engine will not work.

Once you have the SDK up and running and the Whisperfish sources fetched,
it compiles just like any other native Sailfish OS application.

Set the build target 4.6.0.13 and architecture of your choice. The resulting
packages should also work for several older Sailfish OS releases as well:

    sfdk config target=SailfishOS-4.6.0.13-aarch64 build

Then just build it:

    sfdk build

If you want to also build the sharing plugin for SFOS 4.3+, use this command (note the double double dashes):

    sfdk build -- --with shareplugin_v2

For Sailfish 4.2 and older, use `--with shareplugin_v1` instead.

Because of a bug in `sb2`, it is currently not possible to (reliably) build Whisperfish (or any other Rust project) using more than a single thread. This means your compilation is going to take a while, especially the first time. Get yourself some coffee!

However, subsequent compilations (i.e. only Whisperfish code has been changed) tend to build fine with modest threading. This seems to work rather well with `sfdk` at least. You can take advantage of this by defining a `taskset` macro that expands to the [taskset](https://linux.die.net/man/1/taskset) command you want to prefix `cargo build` with. It also removes `-j 1` single-thread parameter:

    sfdk build -- --define "taskset 0x55"

If you get errors (command not found or status 126) at linking stage, make sure that you are not using `~/.cargo/config` to override linkers or compilers.

### Chum and OBS

Whisperfish gained the ability to be built on OBS in late 2024. You can check out the current development package [here](https://build.sailfishos.org/package/show/home:rubdos:whisperfish/Whisperfish).

Chum and OBS don't let us insert e.g. `--with lto` and such, so that needs to be handled differently. Chum sets `%_chum` and the project Whisperfish is built in (manually) sets `%_obs`, so we have [hardwired](https://gitlab.com/whisperfish/whisperfish/-/merge_requests/657/diffs?commit_id=f8bec68a800769c40669136b7d437300852bfbaa) the presence of either of those into `bcond_with` flags.

To mimic OBS build options locally, you can use this command:

    sfdk build -- --define="_obs 1"

At the time of writing, the command above is functionally equivalent with:

    sfdk build -- --with lto --with tools --with vendor

Extra note about vendored build: you need to locally generate (or download from the link above) `vendor.tar.xz` and `vendor.toml` files. You can do them locally like so:

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

Another thing about the `vendor.*` files: they are currently excluded from the git repository on purpose.

### Voice and video calls

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

## Database

Whisperfish uses SQLCipher to store its data. SQLCipher is essentially SQLite with encryption features.
Entering a password when registering Whisperfish makes the database encrypted,
without password it's just a plain SQLite database.

During development it's often handy to have a database or schema at hand.
If you don't want to mess with your Whisperfish database, or even a copy of it,
you can create a plain SQLite database with `create-database.sh`.

## Development environment tips, tricks and hacks

See doc: [Cool hacks for development](doc/dev-env-hacks.md)

## i18n Translations (help wanted)

Whisperfish supports i18n translations and uses
[Text ID Based Translations](http://doc.qt.io/qt-5/linguist-id-based-i18n.html).
For an easy way to help translating, you can join on
[Weblate](https://hosted.weblate.org/engage/whisperfish/).

## License

Before Whisperfish 0.6.0-alpha.1, "the Rust port", Whisperfish was
licensed under the GNU General Public License. Since Whisperfish
0.6.0-alpha.1, Whisperfish links to AGPLv3 code, and as such is a
combined work as meant under clause 13 of the GPLv3.

The original GPLv3 licensed code that is still contained in this
repository, still falls under GPLv3, as per the copyright of Andrew E.
Bruno. This is the original license statement:

Copyright (C) 2016-2018 Andrew E. Bruno

Whisperfish is free software: you can redistribute it and/or modify it
under the terms of the GNU General Public License as published by the
Free Software Foundation, either version 3 of the License, or (at your
option) any later version.

This program is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General
Public License for more details.

You should have received a copy of the GNU General Public License along
with this program. If not, see \<<http://www.gnu.org/licenses/>\>.

This is the license statement since 2019, since Whisperfish
0.6.0-alpha.1.

Copyright (C) 2019-2020 Ruben De Smet, Markus Törnqvist

Whisperfish is free software: you can redistribute it and/or modify it
under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or (at
your option) any later version.

Whisperfish is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero
General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program. If not, see
\<<https://www.gnu.org/licenses/>\>.

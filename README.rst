===============================================================================
Whisperfish
===============================================================================

Whisperfish is a native `Signal <https://www.whispersystems.org/>`_ client for
`Sailfish OS <https://sailfishos.org/>`_. The user interface is heavily based on
the jolla-messages application written by Jolla Ltd.

It is currently mostly working state.  Join our development channel on Matrix
(`#whisperfish:rubdos.be <https://matrix.to/#/#whisperfish:rubdos.be>`_) or
Freenode (#whisperfish) to get in touch, and check
`our wiki <https://gitlab.com/rubdos/whisperfish/-/wikis/home>`_ to see whether
Whisperfish would work for you.

To install, you have two options:

- Releases `from OpenRepos <https://openrepos.net/content/rubdos/whisperfish>`_
- "Nightly" builds from Git development commits.
  The most recent builds can be found in `the Gitlab Package Registry <https://gitlab.com/rubdos/whisperfish/-/packages>`_.

There's no particular reason to install from Git directly.  We push regular updates
to OpenRepos, when they make sense.

Please mind that Whisperfish in still in *alpha condition*, which means that
certain things do not work, other things make the application crash, and I've
heard reports that alpha software can be a cause for dogs eating homework.
You've been warned.
On the other hand, we have many people happily using Whisperfish as daily driver,
and we make up for lacking features in our community support in the aforementioned
Matrix and IRC room.
Please come say hello! We don't bite (we may byte), and we don't eat homework.

-------------------------------------------------------------------------------
Project Status
-------------------------------------------------------------------------------

This project started from a now outdated Go-based SailfishOS client for Signal.
This version, 0.6 and onwards, is a complete rewrite, and uses `libsignal-c-protocol
<https://github.com/signalapp/libsignal-protocol-c>`_ instead.
This means we aim for better maintainability.
It also means the whole SailfishOS app had to be rewritten, and you may want
to make a back-up of your current files if you still come from 0.5. Specifically:

- `.local/share/harbour-whisperfish/` contains all your data.
- `.config/harbour-whisperfish/` contains the apps configuration.

-------------------------------------------------------------------------------
Features
-------------------------------------------------------------------------------

- [x] Registration
- [ ] Contact Discovery
- [x] Direct messages
- [x] Group messages, but not yet `GroupV2 <>`_
- [x] Storing conversations
- [x] Photo attachments
- [x] Encrypted identity and session store
- [x] Encrypted message store
- [x] Advanced user settings
- [ ] Multi-Device support (links with Signal Desktop)
- [ ] Encrypted local attachment store
- [ ] Archiving conversations

Please search the `issue tracker <https://gitlab.com/rubdos/whisperfish/-/issues>`_
before filing any bug report or feature request.
Please upvote issues that are important to you.  We use the vote counter for
determining a feature's priority.

-------------------------------------------------------------------------------
Performance Tips
-------------------------------------------------------------------------------

Whisperfish connects to Signal using Websockets. For a better user experience
try adjusting the power settings on your Jolla to disable late suspend [1].
This should keep the network interfaces up and allow Whisperfish to maintain
websocket connections even when the device is in "sleep". This could
potentially impact your battery life depending on your usage. Otherwise
every time your device goes into deep sleep, the Websocket connection is broken
and you may not receive messages until the next time the OS wakes up and
Whisperfish reconnects.

To disable late suspend and enable "early suspend" run::

    $ mcetool --set-suspend-policy=early    

See here for more information.

1. https://together.jolla.com/question/55056/dynamic-pm-in-jolla/
2. http://talk.maemo.org/showpost.php?p=1401956&postcount=29
3. https://sailfishos.org/wiki/Sailfish_OS_Cheat_Sheet#Blocking_Device_Suspend

-------------------------------------------------------------------------------
Building from source
-------------------------------------------------------------------------------

Whisperfish is built using tooling of the *host* operating system.
Currently tested are Debian and Arch Linux for this purpose.
Fedora, notably, does *not* have the necessary infrastructure for this.
By Whisperfish 1.0.0, we want to use the real SailfishOS SDK, since it offers Rust since version 3.4.

1. Clone the repository::

    $ git clone https://gitlab.com/rubdos/whisperfish

2. Whisperfish is built from the `Platform SDK <https://sailfishos.org/wiki/Platform_SDK>`, outside of the classic ``sb2`` and ``mb2`` environments.
   Refer to the Platform SDK page for installation instructions.
   After that, `install the relevant targets <https://sailfishos.org/wiki/Platform_SDK_Target_Installation>` for the device you are building for,
   e.g.::

    sdk-assistant create SailfishOS-latest-armv7hl http://releases.sailfishos.org/sdk/targets/Sailfish_OS-latest-Sailfish_SDK_Target-armv7hl.tar.7z

4. Still in the SDK chroot use ``sdk-manage`` to install the Sqlite-sqlcipher build dependency, together with some other headers::

   sdk-manage develpkg install SailfishOS-latest-armv7hl sailfish-components-webview-qt5 qt5-qtwebsockets-devel openssl-devel dbus-devel

5. Make a copy of ``dotenv.example`` to ``.env``, adapt it to your configuration and source it.
   Note you can make the ``run.sh`` script log to a file by following the example instructions,
   with the warning that some of the logged information is sensitive.

6. For building on the host, ie. running ``cargo test`` or whatever you may desire, the Debian
   requirements are in ``Dockerfile.builder``, reproduced here::

           $ sudo apt-get install -y \
                   build-essential
                   gcc-i686-linux-gnu g++-i686-linux-gnu binutils-i686-linux-gnu \
                   gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf binutils-arm-linux-gnueabihf \
                   gcc-aarch64-linux-gnu g++-aarch64-linux-gnu binutils-aarch64-linux-gnu \
                   curl \
                   qtbase5-dev \
                   qtbase5-private-dev \
                   qt5-qmake \
                   qtdeclarative5-dev \
                   qttools5-dev-tools qtchooser qt5-default \
                   desktop-file-utils \
                   rpm \
                   cmake \
                   protobuf-compiler \
                   curl \
                   tcl

   You will also be needing some Rust things. Make sure to have a Rust version above 1.48::

           $ rustc --version # Should give something >= 1.48;
           $                 # if not, install something more recent
           $ rustup update
           $ rustup target add armv7-unknown-linux-gnueabihf
           $ cargo install cargo-rpm

7. Configure your cross compilers: copy ``.ci/cargo.toml`` (which is a working file for Debian)
   to ``~/.cargo/config`` (or to ``.cargo/config`` if you do not like this system-wide configuration),
   and edit as necessary for your host operating systems' cross compilers.

8. From here on, you can use cargo to build the project;
   make sure to have the correct targets installed (rustup target) and a C compiler set,
   and to have sourced ``.env``::

    $ cargo build --release --target=armv7-unknown-linux-gnueabihf

   Alternatively, you may use the ``run.sh`` script, which copies the RPM to your device.

   The ``harbour-whisperfish`` executable resides in ``target/[target]/release``.
   You can also use ``cargo rpm`` to build an RPM package,
   note that you need ``rpmtools`` installed on the host system::

    $ cargo install cargo-rpm
    $ cargo rpm build

   The generated RPM can be found in ``target/[target]/release/rpmbuild/RPMS/armv7hl/``.

-------------------------------------------------------------------------------
Testing on the device
-------------------------------------------------------------------------------

The ``run.sh`` script will will source the ``.env`` file and run the build on your device.

-------------------------------------------------------------------------------
Development environment tips, tricks and hacks
-------------------------------------------------------------------------------

See :doc:`Cool hacks for development <doc/dev-env-hacks>`_

-------------------------------------------------------------------------------
i18n Translations (help wanted)
-------------------------------------------------------------------------------

Whisperfish supports i18n translations and uses Text ID Based Translations. See
`here <http://doc.qt.io/qt-5/linguist-id-based-i18n.html>`_ for more info. For
an easy way to help translating, you can join on
`Weblate <https://hosted.weblate.org/engage/whisperfish/>`_.

-------------------------------------------------------------------------------
License
-------------------------------------------------------------------------------

Before Whisperfish 0.6.0-alpha.1, "the Rust port", Whisperfish was licensed under
the GNU General Public License.  Since Whisperfish 0.6.0-alpha.1, Whisperfish links
to AGPLv3 code, and as such is a combined work as meant under clause 13 of the GPLv3.

The original GPLv3 licensed code that is still contained in this repository,
still falls under GPLv3, as per the copyright of Andrew E. Bruno.
This is the original license statement:

Copyright (C) 2016-2018 Andrew E. Bruno

Whisperfish is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with
this program. If not, see <http://www.gnu.org/licenses/>.


This is the license statement since 2019, since Whisperfish 0.6.0-alpha.1.

Copyright (C) 2019-2020 Ruben De Smet, Markus Törnqvist

Whisperfish is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

Whisperfish is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.

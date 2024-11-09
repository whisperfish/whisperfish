// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
pragma Singleton
import QtQuick 2.6
import Nemo.Configuration 1.0
import Sailfish.Silica 1.0 // for StandardPaths

/*!
  This singleton component provides an interface for sailor-emoji,
  a shared emoji implementation for apps on Sailfish OS.

  Current Status:
  - not usable outside of Whisperfish
  - depends on Silica, which is not necessary if sailor-emoji can use
    a C++ plugin
  - uses the implementation provided by sfos-patch-keyboard-color-stock-emojis,
    and falls back to the internal implementation (which is exactly the same)
    - the goal is to remove the internal implementation

  Planned Architecture:
  - sailor-emoji:
    - everything important is packaged separately and installed to
      /usr/share/sailor-emoji
    - sailor-emoji provides a tiny configuration app that lets users
      - download new styles to ~/.local/share/sailor-emoji
      - globally select a style to use
    - sailor-emoji does not provide QML components, only parsing functions,
      style management, and config keys
    - this is preferably to be implemented in a QML plugin in C++
    - sailor-emoji might be Harbour-compatible by itself, as it will provide
      a launcher icon (config app) and technically is not a library

  - keyboard patch:
    - users may install the extended emoji keyboard patch which
      also uses sailor-emoji as "backend"
    - apps (like the keyboard...) may provide their own configuration
      views, which can change the globally defined style:
      - dconf: /apps/sailor-emoji/currentStyle

  - Whisperfish / apps using sailor-emoji:
    - apps using sailor-emoji include this QML component as a proxy
    - if sailor-emoji is not installed or an incompatible version is detected,
      everything silently falls back to plain text
    - apps do not depend on sailor-emoji (necessary for Harbour compatibility)
*/
QtObject {
    id: root

    function parse(text, size) {
        if (_emojiImpl) {
            return _emojiImpl.parseAsMarkup(text, size, _emojiImpl.Style[_style])
        } else {
            return { 'emojiCount': 0, 'plainCount': text.length, 'text': text }
        }
    }

    // --------------------------- private ---------------------------

    property var _emojiImpl: null
    property string _supportedImplVersion: "0.4.0"

    readonly property string _style: _styleConfig.value
    property ConfigurationValue _styleConfig: ConfigurationValue {
        // TODO FIXME No signal is fired when the value changes from
        // outside of Whisperfish, e.g. when the current style is changed
        // in the keyboard config popup. This may be related to #140.
        key: '/apps/sailor-emoji/currentStyle'
        defaultValue: 'openmoji'
    }

    // logging categories are only available since Qt 5.15
    readonly property string logScope: "[Emojify]"

    // TODO Use /usr/share/sailor-emoji/emoji.js.
    readonly property string _implPath: "/usr/share/maliit/plugins/com/jolla/ichthyo_color_emojis/emoji.js"

    Component.onCompleted: {
        try {
            _emojiImpl = Qt.createQmlObject(
                "import QtQuick 2.0; \
                 import '"+_implPath+"' as EmojiJS; \
                 QtObject { function get() { return EmojiJS; } }",
                root, 'EmojiImplementationProxy')
            _emojiImpl = _emojiImpl.get()

            // TODO Move to sailor-emoji.
            _emojiImpl.dataBaseDirectory = StandardPaths.genericData
            console.log(logScope, "implementation loaded")
        } catch(err) {
            _emojiImpl = null
            console.log(logScope, "global implementation could not be loaded")
            console.log(logScope, "trace:", JSON.stringify(err.qmlErrors, null, 2))

            // fall back to internal implementation
            // >>> TODO remove with sailor-emoji
            _emojiImpl = Qt.createQmlObject(
                "import QtQuick 2.0; \
                 import '"+Qt.resolvedUrl("../js/emoji.js")+"' as EmojiJS; \
                 QtObject { function get() { return EmojiJS; } }",
                root, 'EmojiImplementationProxy')
            _emojiImpl = _emojiImpl.get()
            _emojiImpl.dataBaseDirectory = StandardPaths.genericData
            console.log(logScope, "internal implementation loaded")
            // <<< TODO remove with sailor-emoji
        }

        if (_emojiImpl) {
            if (!_emojiImpl.version || String(_emojiImpl.version)[0] !== _supportedImplVersion[0]) {
                console.log(logScope, "incompatible version encountered:",
                            _emojiImpl.version, "vs.", _supportedImplVersion)
                _emojiImpl = null
            }
        }

        if (!_emojiImpl) console.log(logScope, "emoji support disabled")
    }
}

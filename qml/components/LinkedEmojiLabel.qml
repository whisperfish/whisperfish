// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.TextLinking 1.0
import "." // for Emojify singleton

/*!
  This component is a Silica label which can optionally parse text
  - to make phone numbers, email addresses, and URLs clickable
  - and to inject custom emoji icons instead of using the system font for emojis

  Set the \c plainText property to the desired text. Do not change
  the \c text property.

  Important:

  Eliding rich text is not properly supported by Qt. Therefore, the \c elide
  and \c truncationMode properties are disabled. To enable a (costly)
  workaround, set \c enableElide to \c Text.Elide... Make sure both width and
  height are specified either explicitly or through anchors.

  Sometimes eliding still fails to happen at the right position, especially
  if many emojis are involved. Set \c maximumLineCount to make sure the text
  is truncated and does not overflow the label, even though this may strip
  the ellipsis ('...').

  If eliding is enabled, the wrap mode must be \c Text.WrapAnywhere.

  The effective emoji size can be changed through the \c emojiSizeMult property.
  It will be forced to \c 1.0 if eliding is enabled.

  TODO: Replace the wrapped LinkedLabel with a custom text parser that
  supports emojis and Whisperfish-specific links.

  Performance of the current implementation is not optimal because the
  text has to be parsed and layouted multiple times.
*/
Label {
    id: root
    property string plainText
    property real emojiSizeMult: 1.2
    property bool enableEmojis: true
    property bool enableCounts: false // enable only if necessary; performance
    property alias enableElide: elideFixProxy.elide // cf. comments above
    property bool defaultLinkActions: true
    property alias shortenUrl: linkedTextProxy.shortenUrl
    property alias proxy: linkedTextProxy
    property bool bypassLinking: false
    property bool needsRichText: false
    property bool hasSpoilers: false

    // Prefix the rich text with this style to use correct link color
    property string cssStyle: "<style> a { color: " + linkColor + " } </style>"

    readonly property string maybeLinkedText: bypassLinking ? plainText : linkedTextProxy.text

    readonly property int emojiCount: _parsedCountData !== null ? _parsedCountData.emojiCount : 0
    readonly property int plainCharactersCount: _parsedCountData !== null ?
                                                    _parsedCountData.plainCount : plainText.length
    readonly property bool emojiOnly: _parsedCountData !== null ?
                                          (emojiCount > 0 && emojiCount <= emojiOnlyThreshold &&
                                           plainCharactersCount === 0) : false
    property int emojiOnlyThreshold: 5

    readonly property bool _elideEnabled: enableElide !== Text.ElideNone
    readonly property real _effectiveEmojiSize: _elideEnabled ?
                                                    1.0*font.pixelSize :
                                                    emojiSizeMult*font.pixelSize
    property var _parsedEmojiData: enableEmojis ? Emojify.parse(maybeLinkedText,
                                                              _effectiveEmojiSize) : null
    property string _effectiveText: (enableEmojis && _parsedEmojiData !== null) ?
                                        _parsedEmojiData.text :
                                        maybeLinkedText

    // We parse the data a second time without being dependent on the
    // _effectiveEmojiSize property. This allows the emoji count to be used to
    // decide the font size. (Otherwise there's a binding loop on _parsedEmojiData.)
    // We also have to parse the full plain text instead of the elided text
    // to avoid a binding loop through elideFixProxy.font.
    property var _parsedCountData: (enableEmojis && enableCounts) ?
                                       Emojify.parse(plainText, 0) : null

    // shadow elide settings; there is no way to ensure they are set to ...None
    readonly property int truncationMode: 0
    readonly property int elide: 0 // to enable, use enableElide instead

    text: _effectiveText
    textFormat: needsRichText ? Text.RichText : Text.StyledText
    wrapMode: _elideEnabled ? Text.WrapAnywhere : Text.Wrap
    font.pixelSize: Theme.fontSizeMedium
    onLinkActivated: defaultLinkActions ? linkedTextProxy.linkActivated(link) : {}
    linkColor: color

    LinkedText {
        id: linkedTextProxy
        visible: false
        plainText: _elideEnabled ? elideFixProxy.elidedText :
                                   parent.plainText
        linkColor: root.linkColor
    }

    Text {
        id: lineHeightMetrics
        visible: false // enable to verify we have two lines
        maximumLineCount: 2
        lineHeight: root.lineHeight
        lineHeightMode: root.lineHeightMode
        font: root.font
        width: 1 // we have a bug if this does not yield one character per line
        color: Theme.errorColor
        text: 'XXXXXX'
        wrapMode: Text.WrapAnywhere
        property real calcLineHeight: implicitHeight/lineCount
    }

    TextMetrics {
        id: elideFixProxy
        text: _elideEnabled ? plainText : ''
        font: root.font
        elide: Text.ElideNone
        // We have a binding loop if elide is enabled and either
        // root.width or root.height are neither set explicitly nor through anchors.
        // This is because the root label's implicit size depends on its text,
        // which in turn depends on elideFixProxy, which in turn requires a size
        // based on the effective space available.
        elideWidth: _elideEnabled ?
                        root.width * Math.min(
                            Math.floor(root.height/lineHeightMetrics.calcLineHeight),
                            root.maximumLineCount)
                      : 0
    }

    // XXX Label does not support inline JavaScript, so we have to use a MouseArea
    MouseArea {
        anchors.fill: parent
        enabled: root.hasSpoilers
        onClicked: {
            messageLabel.plainText = root.plainText.replace(modelData.spoilerTag, modelData.revealedTag)
            root.hasSpoilers = false
        }
    }
}

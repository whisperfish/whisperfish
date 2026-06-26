// SPDX-FileCopyrightText: 2026 Ruben De Smet
// SPDX-License-Identifier: AGPL-3.0-or-later

import QtQuick 2.6
import Sailfish.Silica 1.0

// Displays a group member's custom "label" as a small pill, mirroring the
// pills shown next to member names in Signal Android/Desktop group views.
//
// The pill contains an optional emoji followed by an optional text label,
// and hides itself entirely when both are empty.
//
// Set \c alignHeight to the height of the element the pill sits next to
// (e.g. a sibling name label) so the pill is vertically centered against it
// when laid out by a positioner such as Row that top-aligns its children.
// When \c alignHeight is 0, the item is only as tall as the pill itself.
Item {
    id: root

    // The member's custom label text (may be empty).
    property string labelText: ""
    // The member's custom label emoji (may be empty).
    property string labelEmoji: ""

    // Height of the line the pill is aligned against. When non-zero, the
    // item occupies this height and centers the pill vertically within it.
    property real alignHeight: 0

    readonly property bool _hasText: root.labelText.length > 0
    readonly property bool _hasEmoji: root.labelEmoji.length > 0
    visible: _hasText || _hasEmoji

    readonly property real _hPadding: Theme.paddingSmall
    readonly property real _vPadding: Theme.paddingSmall / 2

    // The visible pill, sized to its content and vertically centered.
    Item {
        id: pill
        visible: root.visible
        width: visible ? (row.implicitWidth + 2 * root._hPadding) : 0
        height: visible ? (textLabel.implicitHeight + 2 * root._vPadding) : 0
        // Center within the (possibly taller) root item; left-aligned.
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Theme.rgba(Theme.highlightColor, 0.18)
            border {
                width: 1
                color: Theme.rgba(Theme.highlightColor, 0.35)
            }
        }

        Row {
            id: row
            anchors.centerIn: parent
            spacing: (root._hasEmoji && root._hasText) ? Theme.paddingSmall / 2 : 0

            Label {
                id: emojiLabel
                visible: root._hasEmoji
                text: root.labelEmoji
                font.pixelSize: Theme.fontSizeTiny
                color: Theme.highlightColor
            }

            Label {
                id: textLabel
                visible: root._hasText
                text: root.labelText
                font.pixelSize: Theme.fontSizeTiny
                color: Theme.highlightColor
                elide: Text.ElideRight
            }
        }
    }

    implicitWidth: visible ? pill.width : 0
    implicitHeight: visible ? (root.alignHeight > 0 ? root.alignHeight : pill.height) : 0
    width: implicitWidth
    height: implicitHeight
}

// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later

// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

BackgroundItem {
    id: root

    implicitWidth: Math.max(label.width, minimumWidth)
    implicitHeight: label.height + (enabled ? backgroundGrow : 0)
    width: Math.min(implicitWidth, maximumWidth)
    height: implicitHeight

    _backgroundColor: "transparent"

    property QtObject recipient

    property string source
    property bool outbound
    property bool isInGroup
    property real maximumWidth
    property real minimumWidth

    property alias horizontalAlignment: label.horizontalAlignment
    property alias radius: roundedRectangle.radius
    property real backgroundGrow: Theme.paddingMedium

    onClicked: !outbound ? pageStack.push(Qt.resolvedUrl("../pages/RecipientProfilePage.qml"), { recipient: recipient, groupContext: isInGroup } ) : {}

    LinkedEmojiLabel {
        id: label

        highlighted: root.highlighted
        plainText: outbound
                   ? //: Name shown when replying to own messages
                     //% "You"
                     qsTrId("whisperfish-sender-name-label-outgoing")
                    : source
        maximumLineCount: 1
        wrapMode: Text.NoWrap
        font.pixelSize: Theme.fontSizeExtraSmall
        font.bold: true
        linkColor: color
        color: Qt.tint(highlighted ? Theme.highlightColor : Theme.primaryColor,
                       '#'+Qt.md5(source).substr(0, 6)+'0F')
        defaultLinkActions: false
    }

    RoundedRect {
        id: roundedRectangle

        visible: root.enabled

        x: -backgroundGrow
        y: -backgroundGrow
        height: label.height + 2 * backgroundGrow
        width: parent.width + 2 * backgroundGrow

        color: down ? Theme.highlightBackgroundColor : "transparent"
        opacity: Theme.opacityFaint
        roundedCorners: bottomLeft | bottomRight | (outbound ? topRight : topLeft)
        radius: Theme.paddingLarge * 0.75
    }
}

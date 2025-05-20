// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: root
    property string source: ''
    property QtObject recipient
    property bool outbound: false
    property bool isInGroup: false
    property real maximumWidth: metrics.width

    property alias horizontalAlignment: label.horizontalAlignment
    property bool enableBackground: true
    property alias backgroundItem: bgRect
    // \c backgroundGrow sets how far the background grows over the item's boundaries
    property real backgroundGrow: Theme.paddingMedium

    property alias down: bgItem.down
    property alias highlighted: bgItem.highlighted

    property bool defaultClickAction: true
    signal clicked(var mouse)

    implicitWidth: Math.min(metrics.width, maximumWidth)
    implicitHeight: (visible && opacity > 0.0) ? metrics.height : 0
    width: implicitWidth
    height: implicitHeight
    enabled: visible

    onClicked: !outbound ? pageStack.push(Qt.resolvedUrl("../pages/RecipientProfilePage.qml"), { recipient: recipient, groupContext: isInGroup } ) : {}

    TextMetrics {
        id: metrics
        font: label.font
        text: label.plainText
    }

    BackgroundItem {
        id: bgItem
        enabled: enableBackground
        visible: enableBackground && root.height > 0
        onClicked: root.clicked(mouse)
        _backgroundColor: "transparent"
        anchors {
            fill: parent
            margins: -backgroundGrow
        }

        RoundedRect {
            id: bgRect
            color: down ? Theme.highlightBackgroundColor : "transparent"
            opacity: Theme.opacityFaint
            roundedCorners: bottomLeft | bottomRight | (outbound ? topRight : topLeft)
            anchors.fill: parent
            radius: Theme.paddingLarge
        }
    }

    LinkedEmojiLabel {
        id: label
        highlighted: root.highlighted
        plainText: outbound ?
                       //: Name shown when replying to own messages
                       //% "You"
                       qsTrId("whisperfish-sender-name-label-outgoing") :
                       source
        width: parent.implicitWidth
        height: parent.implicitHeight
        maximumLineCount: 1
        wrapMode: Text.NoWrap
        font.pixelSize: Theme.fontSizeExtraSmall
        font.bold: true
        linkColor: color
        color: Qt.tint(highlighted ? Theme.highlightColor : Theme.primaryColor,
                       '#'+Qt.md5(source).substr(0, 6)+'0F')
        defaultLinkActions: false
    }
}

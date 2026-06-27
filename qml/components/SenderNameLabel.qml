// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

BackgroundItem {
    id: root

    // The sender's custom group member label, shown as a pill to the right of
    // the name. Only rendered for incoming group messages (see the pill's
    // `visible` binding); outbound messages and direct sessions leave the
    // properties empty, so the pill stays hidden.
    property string labelText: ""
    property string labelEmoji: ""

    readonly property real _pillWidth: memberLabelPill.visible
        ? Theme.paddingSmall + memberLabelPill.width : 0

    implicitWidth: Math.max(label.width + _pillWidth, minimumWidth)
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

    onClicked: !outbound
                    ? pageStack.push(
                        Qt.resolvedUrl("../pages/RecipientProfilePage.qml"),
                        { recipient: recipient, groupContext: isInGroup })
                    : {}

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
        color: !!recipient
                    ? Qt.tint(highlighted ? Theme.highlightColor : Theme.primaryColor,
                        '#'+Qt.md5(recipient.uuid).substr(0, 6)+'0F')
                    : highlighted ? Theme.highlightColor : Theme.primaryColor
        defaultLinkActions: false
    }

    MemberLabelPill {
        id: memberLabelPill
        anchors.left: label.right
        anchors.leftMargin: Theme.paddingSmall
        anchors.verticalCenter: label.verticalCenter
        alignHeight: label.height
        labelText: root.labelText
        labelEmoji: root.labelEmoji
        // Pills belong to incoming group senders only.
        visible: !root.outbound && (root.labelText.length > 0 || root.labelEmoji.length > 0)
    }

    RoundedRect {
        id: roundedRectangle

        x: -backgroundGrow
        y: -backgroundGrow
        height: label.height + 2 * backgroundGrow
        width: parent.width + 2 * backgroundGrow
        roundedCorners: bottomLeft | bottomRight | (outbound ? topRight : topLeft)
        radius: Theme.paddingLarge * 0.75

        visible: root.enabled
        color: down ? Theme.highlightBackgroundColor : "transparent"
        opacity: Theme.opacityFaint
    }
}

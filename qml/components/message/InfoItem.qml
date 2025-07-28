// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

// This component must be a child of MessageDelegate.
Row {
    id: infoRow
    height: Math.max(infoLabel.height, debugLabel.height, showMoreColumn.height)

    Column {
        id: showMoreColumn
        Row {
            visible: showExpand
            width: visible ? implicitWidth : 0
            spacing: Theme.paddingSmall
            layoutDirection: isOutbound ? Qt.LeftToRight : Qt.RightToLeft

            Item { width: Theme.paddingSmall; height: parent.height }
            Label {
                font.pixelSize: Theme.fontSizeExtraSmall
                text: "\u2022 \u2022 \u2022" // three dots
            }
            Label {
                text: isExpanded ?
                        //: Hint for very long messages, while expanded
                        //% "show less"
                        qsTrId("whisperfish-message-show-less") :
                        //: Hint for very long messages, while not expanded
                        //% "show more"
                        qsTrId("whisperfish-message-show-more")
                font.pixelSize: Theme.fontSizeExtraSmall
            }
        }
    }

    Label {
        id: privacyIcon
        anchors.verticalCenter: parent.verticalCenter
        visible: SettingsBridge.debug_mode
        width: visible ? implicitWidth : 0
        height: infoLabel.height
        color: unidentifiedSender ? "green" : "red"
        font.pixelSize: Theme.fontSizeTiny
        text: '🔒'
        verticalAlignment: Text.AlignVCenter
    }

    ExpirationIndicator {
        id: expityIndicator
        anchors.verticalCenter: parent.verticalCenter
        height: infoLabel.height * 0.75
        width: isRunning ? 0.65 * height : 0.0
        visible: width > 0.0

        Behavior on width { NumberAnimation { duration: 150 } }

        expiresIn: modelData.expiresIn != null ? modelData.expiresIn : -1
        expiryStarted: modelData.expiryStarted

        itemColor: isOutbound ?
                   (highlighted ? Theme.secondaryHighlightColor :
                                  Theme.secondaryHighlightColor) :
                   (highlighted ? Theme.secondaryHighlightColor :
                                  Theme.secondaryColor)
    }

    Item {
        visible: !statusIcon.visible && expityIndicator.visible
        height: infoLabel.height
        width: Theme.paddingSmall
    }

    HighlightImage {
        id: statusIcon
        visible: isOutbound
        width: visible ? infoLabel.height : 0
        height: infoLabel.height
        anchors.verticalCenter: parent.verticalCenter
        color: infoLabel.color
        source: {
            if (!hasData) "../../../icons/icon-s-queued.png" // cf. below
            if (modelData.read > 0) "../../../icons/icon-s-read.png"
            else if (modelData.delivered > 0) "../../../icons/icon-s-received.png"
            else if (modelData.sent) "../../../icons/icon-s-sent.png"
            else if (modelData.queued) "../../../icons/icon-s-queued.png"
            // TODO check if SFOS 4 has "image://theme/icon-s-blocked" (3.4 doesn't)
            else if (modelData.failed) "../../../icons/icon-s-failed.png"
            // If none of the above options are true, then we assume failure.
            else "../../../icons/icon-s-failed.png"
        }
    }

    Label {
        id: infoLabel
        anchors.verticalCenter: parent.verticalCenter
        text: hasData ?
                  (modelData.timestamp ?
                       Format.formatDate(modelData.timestamp, Formatter.TimeValue) :
                       //: Placeholder note if a message doesn't have a timestamp (which must not happen).
                       //% "no time"
                       qsTrId("whisperfish-message-no-timestamp")) :
                  '' // no message to show
        horizontalAlignment: isOutbound ? Text.AlignRight : Text.AlignLeft // TODO make configurable
        font.pixelSize: Theme.fontSizeExtraSmall // TODO make configurable
        color: isOutbound ?
                   (highlighted ? Theme.secondaryHighlightColor :
                                  Theme.secondaryHighlightColor) :
                   (highlighted ? Theme.secondaryHighlightColor :
                                  Theme.secondaryColor)
    }

    Label {
        id: debugLabel
        anchors.verticalCenter: parent.verticalCenter
        visible: SettingsBridge.debug_mode
        width: visible ? implicitWidth : 0
        text: (visible && modelData) ? " [%1] ".arg(modelData.id) : ""
        color: infoLabel.color
        font.pixelSize: Theme.fontSizeExtraSmall
    }
}

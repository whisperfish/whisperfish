// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "attachment"

BackgroundItem {
    id: root
    // 'attachments' is expected as a list of objects: [{data: path, type: mimetype}, ...]
    property alias messageId: quotedMessage.messageId
    property bool showCloseButton: true
    property bool showBackground: false
    property real contentPadding: Theme.paddingMedium

    property alias maximumWidth: senderNameLabel.maximumWidth
    property alias horizontalAlignment: textLabel.horizontalAlignment
    property alias backgroundItem: bgRect

    property bool _hasSpoilers: quotedMessage.valid ? quotedMessage.hasSpoilers : false
    property bool _hasStrikeThrough: quotedMessage.valid ? quotedMessage.hasStrikeThrough : false

    readonly property bool shown: quotedMessage.valid && visible
    readonly property bool hasAttachments: quotedMessage.thumbsAttachmentsCount > 0

    implicitWidth: shown ? Math.min(Math.max(senderNameLabel.implicitWidth+2*contentPadding,
                                             metrics.width), maximumWidth) : 0
    implicitHeight: shown ? Math.max(quoteColumn.height, attachThumb.height) : 0
    height: implicitHeight
    width: implicitWidth
    _backgroundColor: "transparent"

    signal closeClicked(var mouse)

    Message {
        id: quotedMessage
        app: AppState
        // messageId through alias above
    }

    Recipient {
        id: sender
        app: AppState
        recipientId: quotedMessage.senderRecipientId ? quotedMessage.senderRecipientId : -1
    }

    TextMetrics {
        id: metrics
        font: textLabel.font
        text: textLabel.plainText
    }

    HighlightImage {
        id: closeButton
        visible: shown && showCloseButton
        // HighlightImage with separate MouseArea instead of IconButton
        // for finer control over size and placement
        anchors {
            verticalCenter: parent.verticalCenter
            right: quoteColumn.left; rightMargin: Theme.paddingMedium
        }
        width: visible ? Theme.iconSizeSmallPlus : 0
        height: width
        horizontalAlignment: Image.AlignHCenter
        verticalAlignment: Image.AlignVCenter
        source: "../../icons/icon-s-close.png"
        highlighted: closeButtonArea.pressed || root.down

        MouseArea {
            id: closeButtonArea
            anchors.centerIn: parent
            width: 3*Theme.iconSizeSmall
            height: width
            onClicked: closeClicked(mouse)
        }
    }

    RoundedRect {
        id: bgRect
        visible: shown && showBackground
        color: down ? Theme.highlightBackgroundColor :
                      Theme.rgba(Theme.secondaryColor, Theme.opacityFaint)
        opacity: Theme.opacityFaint
        roundedCorners: allCorners
        anchors.fill: parent
        radius: Theme.paddingMedium
    }

    Column {
        id: quoteColumn
        visible: shown
        topPadding: padding-0.9*Theme.paddingSmall // remove excessive top padding
        spacing: Theme.paddingSmall
        height: childrenRect.height + 2*padding
        anchors {
            left: parent.left
            leftMargin: showCloseButton ? closeButton.width+Theme.paddingMedium :
                                          contentPadding
            right: attachThumb.left
            rightMargin: contentPadding
        }

        Item { height: 1; width: parent.width } // spacing

        SenderNameLabel {
            id: senderNameLabel
            source: quotedMessage.outgoing ?
                // Reused from main.qml; "You"
                qsTrId("whisperfish-sender-name-label-outgoing") :
                (sender.valid ?
                    getRecipientName(sender.e164, sender.externalId, sender.name, false) :
                    //: Text shown on quotes when the sender of a quote is unknown
                    //% "Unknown sender"
                    qsTrId("whisperfish-quoted-message-unknown-sender"))
            defaultClickAction: false
            anchors { left: parent.left; right: parent.right }
            maximumWidth: parent.width
            horizontalAlignment: root.horizontalAlignment
            highlighted: root.highlighted
            enableBackground: false
        }

        // Extra Item required for Column containing LinkedEmojiLabel <- OpacityRampEffect
        Item {
            anchors { left: parent.left; right: parent.right }
            height: textLabel.height
            LinkedEmojiLabel {
                id: textLabel
                enabled: false
                anchors { left: parent.left; right: parent.right }
                height: Math.min(implicitHeight, Theme.itemSizeMedium)
                verticalAlignment: Text.AlignTop
                horizontalAlignment: Text.AlignLeft
                plainText: (quotedMessage.valid && quotedMessage.message.trim() !== '')
                            ? ((needsRichText ? cssStyle : '') + quotedMessage.styledMessage)
                            : (
                                (quotedMessage.valid && quotedMessage.attachments.length > 0)
                                //: Placeholder text if quoted message preview contains no text, only attachments
                                //% "Attachment"
                                ? qsTrId("whisperfish-quoted-message-preview-attachment")
                                : ''
                            )

                clip: true
                bypassLinking: true
                needsRichText: _hasSpoilers || _hasStrikeThrough
                hasSpoilers: _hasSpoilers

                font.pixelSize: Theme.fontSizeExtraSmall
                emojiSizeMult: 1.2
                color: root.highlighted ? Theme.secondaryHighlightColor :
                                        Theme.secondaryColor
                linkColor: color
                defaultLinkActions: false
                onLinkActivated: root.clicked(null)
            }

            OpacityRampEffect {
                offset: 0.8
                slope: 5
                sourceItem: textLabel
                enabled: textLabel.contentHeight > textLabel.height
                direction: OpacityRamp.TopToBottom
            }
        }
    }

    AttachmentItemBase {
        id: attachThumb
        anchors {
            right: parent.right; rightMargin: 0
            verticalCenter: parent.verticalCenter
        }
        width: attach === null ? 0 : Theme.itemSizeMedium
        height: width
        attach: hasAttachments ? quotedMessage.thumbsAttachments.get(0) : null
        enabled: hasAttachments
        layer.enabled: true
        layer.smooth: true
        layer.effect: RoundedMask {
            // TODO the corners may have to be adapted for different use cases...
            roundedCorners: allCorners
            radius: Theme.paddingSmall
        }
    }
}

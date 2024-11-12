// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../attachment"
import "../../js/attachment.js" as Attachment
import ".."

// This component must be a child of MessageDelegate.
Loader {
    id: root
    readonly property var thumbsRe: /^(image|video)\//

    property alias messageId: message.messageId

    Message {
        id: message
        app: AppState
    }

    property alias thumbsAttachments: message.thumbsAttachments
    property alias detailAttachments: message.detailAttachments
    property int thumbsAttachmentCount: message.thumbsAttachments.count
    property int detailAttachmentCount: message.detailAttachments.count
    property real thumbsHeight: thumbsAttachmentCount > 0 ? Math.min(2*Theme.itemSizeExtraLarge, width) : 0
    property real detailItemHeight: Theme.itemSizeMedium
    property real detailHeight: detailAttachmentCount > 0 ? Math.min(maxDetails, detailAttachmentCount)*detailItemHeight : 0
    property real spacing: (thumbsAttachments > 0 && detailAttachmentCount > 0) ? Theme.paddingMedium : 0

    property bool cornersOutbound: false
    property bool cornersQuoted: false

    property string audioTranscription: detailAttachmentCount > 0 ? valueOrEmptyString(detailAttachments.get(0).transcription) : ""

    readonly property int maxDetails: 2
    readonly property int maxThumbs: 5

    signal pressAndHold(var mouse)
    asynchronous: true
    onPressAndHold: handleExternalPressAndHold(mouse)

    // TODO adapt size to screen orientation, i.e. reduce in horizontal mode
    width: 2*Theme.itemSizeExtraLarge
    height: {
        if (!enabled) 0
        else thumbsHeight+detailHeight+spacing
    }

    // TODO Show retry icon
    // TODO Show list of all attachments for >5 images or >2 documents
    // TODO Show image details etc., and actions, on an attached page
    // TODO Stickers: what mime type? custom/variable size?

    sourceComponent: Component {
        Item {
            opacity: highlighted ? Theme.opacityHigh : 1.0
            layer.enabled: true
            layer.effect: RoundedMask {
                id: roundedMask
                roundedCorners: cornersQuoted ? allCorners : (bottomLeft | bottomRight |
                                                              (cornersOutbound ? topRight : topLeft))
                radius: Theme.paddingMedium
            }

            Column {
                width: parent.width
                height: thumbsHeight+detailHeight+root.spacing
                spacing: 0
                Loader {
                    width: parent.width
                    height: thumbsHeight
                    asynchronous: true
                    sourceComponent: {
                        if (thumbsAttachmentCount === 0) null
                        else if (thumbsAttachmentCount === 1) mediaComponent_1
                        else if (thumbsAttachmentCount === 2) mediaComponent_2
                        else if (thumbsAttachmentCount === 3) mediaComponent_3
                        else if (thumbsAttachmentCount === 4) mediaComponent_4
                        else if (thumbsAttachmentCount >= 5) mediaComponent_5_plus
                    }
                }

                Item { width: parent.width; height: root.spacing }

                Loader {
                    asynchronous: true
                    width: parent.width
                    height: detailHeight
                    sourceComponent: detailComponent
                }
            }
        }
    }

    Component {
        id: mediaComponent_1
        AttachmentThumbnail {
            anchors.fill: parent
            attachments: thumbsAttachments
            index: 0
            message: modelData
            onPressAndHold: root.pressAndHold(mouse)
            enabled: !listView.isSelecting
        }
    }

    Component {
        id: mediaComponent_2
        Row {
            enabled: !listView.isSelecting
            AttachmentThumbnail {
                width: parent.width/2; height: parent.height
                attachments: thumbsAttachments
                index: 0
                message: modelData
                onPressAndHold: root.pressAndHold(mouse)
            }
            AttachmentThumbnail {
                width: parent.width/2; height: parent.height
                attachments: thumbsAttachments
                index: 1
                message: modelData
                onPressAndHold: root.pressAndHold(mouse)
            }
        }
    }

    Component {
        id: mediaComponent_3
        Row {
            enabled: !listView.isSelecting
            AttachmentThumbnail {
                width: parent.width/2; height: parent.height
                attachments: thumbsAttachments
                index: 0
                message: modelData
                onPressAndHold: root.pressAndHold(mouse)
            }

            Column {
                width: parent.width/2; height: parent.height
                AttachmentThumbnail {
                    width: parent.width; height: parent.height/2
                    attachments: thumbsAttachments
                    index: 1
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
                AttachmentThumbnail {
                    width: parent.width; height: parent.height/2
                    attachments: thumbsAttachments
                    index: 2
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
            }
        }
    }

    Component {
        id: mediaComponent_4
        Row {
            enabled: !listView.isSelecting
            Column {
                width: parent.width/2; height: parent.height
                AttachmentThumbnail {
                    width: parent.width; height: parent.height/2
                    attachments: thumbsAttachments
                    index: 0
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
                AttachmentThumbnail {
                    width: parent.width; height: parent.height/2
                    attachments: thumbsAttachments
                    index: 1
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
            }
            Column {
                width: parent.width/2; height: parent.height
                AttachmentThumbnail {
                    width: parent.width; height: parent.height/2
                    attachments: thumbsAttachments
                    index: 2
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
                AttachmentThumbnail {
                    width: parent.width; height: parent.height/2
                    attachments: thumbsAttachments
                    index: 3
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
            }
        }
    }

    Component {
        id: mediaComponent_5_plus
        Column {
            enabled: !listView.isSelecting
            Row {
                width: parent.width; height: parent.height/5*3
                AttachmentThumbnail {
                    width: parent.width/2; height: parent.height
                    attachments: thumbsAttachments
                    index: 0
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
                AttachmentThumbnail {
                    width: parent.width/2; height: parent.height
                    attachments: thumbsAttachments
                    index: 1
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
            }
            Row {
                width: parent.width; height: parent.height/5*2
                AttachmentThumbnail {
                    width: parent.width/3; height: parent.height
                    attachments: thumbsAttachments
                    index: 2
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
                AttachmentThumbnail {
                    width: parent.width/3; height: parent.height
                    attachments: thumbsAttachments
                    index: 3
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)
                }
                AttachmentThumbnail {
                    id: showMoreThumb
                    width: parent.width/3; height: parent.height
                    attachments: thumbsAttachments
                    index: 4
                    message: modelData
                    onPressAndHold: root.pressAndHold(mouse)

                    OpacityRampEffect {
                        sourceItem: thumbsOverlay
                        direction: OpacityRamp.BottomToTop
                        offset: 0.0
                        slope: 0.5
                    }

                    Rectangle {
                        id: thumbsOverlay
                        visible: thumbsAttachmentCount > maxThumbs
                        anchors.fill: parent
                        color: Theme.highlightDimmerColor
                        opacity: parent.highlighted ? 0.7 : 0.85

                        Label {
                            highlighted: showMoreThumb.highlighted
                            anchors.fill: parent
                            //: Label hinting at more attachments than are currently shown. Read as "and %n more".
                            //% "+%n"
                            text: qsTrId("whisperfish-attachments-plus-n", thumbsAttachmentCount - maxThumbs)
                            fontSizeMode: Text.Fit
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                }
            }
        }
    }

    Component {
        id: detailComponent
        Column {
            id: detailColumn
            enabled: listView != null && !listView.isSelecting

            function componentForMime(detailAttachment) {
                if (/^audio\//.test(detailAttachment.type) && !Attachment.isPlaylist(detailAttachment.data)) return detail_audioComponent
                else if (/^text\/(x-)?vcard/.test(detailAttachment.type)) return detail_contactComponent
                /* else if (detailAttachment.type === 'text/x-signal-plain') return null */
                else return detail_fileComponent
            }

            Loader {
                property int currentAttachmentIndex: 0
                asynchronous: true
                width: parent.width
                height: parent.height/Math.min(maxDetails, detailAttachmentCount)
                sourceComponent: detailAttachmentCount >= 1
                                ? parent.componentForMime(detailAttachments.get(currentAttachmentIndex))
                                : null
            }

            Item {
                width: parent.width
                height: showMoreDetail.sourceComponent != null ? parent.height/maxDetails : 0

                Loader {
                    id: showMoreDetail
                    asynchronous: true
                    anchors.fill: parent
                    property int currentAttachmentIndex: 1
                    opacity: detailOverlay.visible ? Theme.opacityFaint : 1.0
                    sourceComponent: detailAttachmentCount >= maxDetails
                                    ? detailColumn.componentForMime(detailAttachments.get(currentAttachmentIndex))
                                    : null
                }

                OpacityRampEffect {
                    enabled: detailOverlay.visible
                    sourceItem: detailOverlay
                    direction: OpacityRamp.BottomToTop
                    offset: -0.1
                    slope: 1.0
                }

                Rectangle {
                    id: detailOverlay
                    visible: detailAttachmentCount > maxDetails
                    color: Theme.highlightDimmerColor
                    anchors.fill: showMoreDetail
                }

                Label {
                    visible: detailOverlay.visible
                    highlighted: (showMoreDetail.item && showMoreDetail.item.highlighted) ? true : undefined
                    anchors { fill: detailOverlay; margins: Theme.paddingMedium }
                    //: Note if some message attachments are hidden instead of being shown inline
                    //% "and %n more"
                    text: qsTrId("whisperfish-attachments-loader-show-more",
                                 detailAttachmentCount - maxDetails+1)
                    fontSizeMode: Text.Fit
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
    }

    Component {
        id: detail_contactComponent
        AttachmentItemContact {
            attachments: detailAttachments
            index: currentAttachmentIndex
            onPressAndHold: root.pressAndHold(mouse)
        }
    }

    Component {
        id: detail_audioComponent
        AttachmentItemAudio {
            attachments: detailAttachments
            index: currentAttachmentIndex
            recipientId: message.senderRecipientId
            onPressAndHold: root.pressAndHold(mouse)
        }
    }

    Component {
        id: detail_fileComponent
        AttachmentItemFile {
            attachments: detailAttachments
            index: currentAttachmentIndex
            recipientId: message.senderRecipientId
            onPressAndHold: root.pressAndHold(mouse)
        }
    }
}

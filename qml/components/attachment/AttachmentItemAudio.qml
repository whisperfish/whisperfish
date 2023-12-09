// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import QtMultimedia 5.6
import be.rubdos.whisperfish 1.0

// TODO distinguish between voice notes and attached audio files
AttachmentItemBase {
    id: item
    property var recipientId: -1

    Recipient {
        id: recipient
        app: AppState
        recipientId: item.recipientId
    }

    RustleGraph {
        id: rustlegraph
        app: AppState
        attachmentId: attach.id

        // When the graph becomes interactive for scrolling, we might want to make these into primaryColor and  + secondaryColor
        pastColor: Theme.highlightColor
        futureColor: Theme.secondaryHighlightColor

        width: rustlegraphImage.width
        height: rustlegraphImage.height
        // timestamp: audioMessage.position ? (audioMessage.position / 1000.0) : 0.0
    }

    // Qt 5.9+ can just use the notifyInterval of Audio, but we have to trick the animation into being smooth.
    Timer {
        running: audioMessage.playbackState == Audio.PlayingState
        repeat: true
        interval: 20 // ms, 50fps
        onTriggered: {
            rustlegraph.timestamp = audioMessage.position / 1000.
        }
    }

    onClicked: {
        if (_effectiveEnableClick) {
            pageStack.push(Qt.resolvedUrl('../../pages/ViewAudioPage.qml'), {
                title: recipientId > -1 ? recipient.name : "",
                subtitle: attach.is_voice_note
                    //: Page header subtitle for a voice note
                    //% "Voice note"
                    ? qsTrId('whisperfish-quoted-message-preview-voice-note')
                    // Translated in QuotedMessagePreview.qml
                    : qsTrId('whisperfish-quoted-message-preview-attachment'),
                'titleOverlay.subtitleItem.wrapMode': SettingsBridge.debug_mode ? Text.Wrap : Text.NoWrap,
                path: attach.data,
                attachmentId: attach.id,
                isViewOnce: false, // TODO: Implement attachment can only be viewed once
                attachment: attach,
            })
        }
    }

    Audio {
        id: audioMessage
        source: attach.data
        property string durationTenths: ""
        onDurationChanged: durationTenths = (
            duration > 0
            ? " (" +  Math.round(duration / 1000) + "s)"
            : durationTenths
        )
        // Qt 5.9+
        // notifyInterval: 20 // ms
    }

    Row {
        id: attachmentRow
        anchors {
            left: parent.left
            right: parent.right
        }
        Column {
            id: playPauseColumn
            IconButton {
                width: item.height
                height: item.height
                icon.source: audioMessage.playbackState === Audio.PlayingState
                        ? "image://theme/icon-m-pause"
                        : "image://theme/icon-m-play"
                onClicked: audioMessage.playbackState === Audio.PlayingState
                           ? audioMessage.pause()
                           : audioMessage.play()
                onPressAndHold: audioMessage.stop()
                clip: true
                Rectangle {
                    z: -2
                    anchors { fill: parent; margins: -parent.width/2 }
                    rotation: 45
                    gradient: Gradient {
                        GradientStop { position: 0.0; color: "transparent" }
                        GradientStop { position: 0.4; color: "transparent" }
                        GradientStop { position: 1.0; color: Theme.rgba(Theme.secondaryColor, 0.1) }
                    }
                }

                IconButton {
                    z: -1
                    anchors {
                        top: parent.top
                        right: parent.right
                    }
                    enabled: false
                    visible: attach.is_voice_note
                    icon.source: "image://theme/icon-cover-unmute"
                    width: parent.height * 0.3
                    height: width
                    icon.width: width
                    icon.height: width
                    highlighted: parent.highlighted
                }
            }
        }

        Column {
            Item {
                height: attachmentRow.height
                width: attachmentRow.width - playPauseColumn.width - Theme.paddingSmall
                Image {
                    id: rustlegraphImage
                    fillMode: Image.PreserveAspectFit
                    source: "image://rustlegraph/" + rustlegraph.imageId
                    anchors.fill: parent
                }
            }
        }
    }
}

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
        attachmentId: attach.is_downloaded ? attach.id : -1

        // When the graph becomes interactive for scrolling, we might want to make these into primaryColor and  + secondaryColor
        pastColor: Theme.highlightColor
        futureColor: Theme.secondaryHighlightColor

        width: rustlegraphImage.width
        height: rustlegraphImage.height
        // timestamp: audioMessage.position ? (audioMessage.position / 1000.0) : 0.0
    }

    // Qt 5.9+ can just use the notifyInterval of MediaPlayer, but we have to trick the animation into being smooth.
    Timer {
        running: attach.is_downloaded && audioMessage.playbackState == MediaPlayer.PlayingState
        repeat: true
        interval: 20 // ms, 50fps
        onTriggered: rustlegraph.timestamp = audioMessage.position / 1000.
    }

    Timer {
        running: attach.is_downloaded && audioMessage.playbackState == MediaPlayer.PlayingState

        interval: 100 // ms, 10fps
        property int seconds: 0
        property int minutes: 0
        property int hours: 0
        property var newText: ""
        onTriggered: {
            seconds = Math.round((audioMessage.duration - audioMessage.position) / 1000)
            minutes = Math.floor(seconds / 60) % 60
            hours = Math.floor(seconds / 3600) % 60
            seconds = seconds % 60

            newText = (hours > 0 ? (hours + ":") : "")
                + (minutes > 9 ? "" : "0") + minutes + ":"
                + (seconds > 9 ? "" : "0") + seconds

            if(newText != playbackLabel.text) {
                playbackLabel.text = newText
            }
        }
    }

    onClicked: {
        if (_effectiveEnableClick) {
            pageStack.push(Qt.resolvedUrl('../../pages/ViewAudioPage.qml'), {
                title: recipientId > -1 ? recipient.name : "",
                subtitle: attach.is_voice_note
                    //: Page header subtitle for a voice note
                    //% "Voice Message"
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

    MediaPlayer {
        id: audioMessage
        source: attach.is_downloaded ? attach.data : ""
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
                icon.width: item.height * 0.6
                icon.height: item.height * 0.6
                icon.source: attach.is_downloaded
                    ? ( audioMessage.playbackState === MediaPlayer.PlayingState
                        ? "../../../icons/pause.png"
                        : "../../../icons/play.png" )
                    : (attach.can_retry ? 'image://theme/icon-s-cloud-download' : '')
                onClicked: {
                    if (attach.can_retry) {
                        ClientWorker.fetchAttachment(attach.id)
                        return;
                    }
                    if (audioMessage.playbackState === MediaPlayer.PlayingState) {
                        audioMessage.pause();
                    } else {
                        audioMessage.play();
                    }
                }

                onPressAndHold: audioMessage.stop()
                clip: true

                BusyIndicator {
                    id: downloadingBusyIndicator
                    running: attach.is_downloading
                    anchors.centerIn: parent
                    size: BusyIndicatorSize.Medium
                }

                Label {
                    id: downloadingLabel
                    visible: downloadingBusyIndicator.running
                    text: Math.round(attach.downloaded_percentage) + " %"
                    anchors.centerIn: parent
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.highlightColor
                }

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

                Label {
                    id: playbackLabel
                    anchors {
                        horizontalCenter: parent.horizontalCenter
                        bottom: parent.bottom
                    }
                    font.pixelSize: Theme.fontSizeTiny
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

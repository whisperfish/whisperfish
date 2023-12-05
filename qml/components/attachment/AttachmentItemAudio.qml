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

    onClicked: {
        if (_effectiveEnableClick) {
            pageStack.push(Qt.resolvedUrl('../../pages/ViewAudioPage.qml'), {
                title: recipientId > -1 ? recipient.name : "",
                // Translated in QuotedMessagePreview.qml
                subtitle: qsTrId('whisperfish-quoted-message-preview-attachment'),
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
    }
    Row {
        id: attachmentRow
        anchors {
            left: parent.left; right: parent.right
            verticalCenter: parent.verticalCenter
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
                clip: true
                Rectangle {
                    z: -1
                    anchors { fill: parent; margins: -parent.width/2 }
                    rotation: 45
                    gradient: Gradient {
                        GradientStop { position: 0.0; color: "transparent" }
                        GradientStop { position: 0.4; color: "transparent" }
                        GradientStop { position: 1.0; color: Theme.rgba(Theme.secondaryColor, 0.1) }
                    }
                }
            }
        }

        Column {
            id: stopButton
            IconButton {
                width: item.height
                height: item.height
                enabled: audioMessage.position > 0
                         && audioMessage.position < audioMessage.duration
                         && audioMessage.playbackState == Audio.PlayingState
                icon.source: "image://theme/icon-m-stop"
                onClicked: audioMessage.stop()
                clip: true
                Rectangle {
                    z: -1
                    anchors { fill: parent; margins: -parent.width/2 }
                    rotation: 45
                    gradient: Gradient {
                        GradientStop { position: 0.0; color: "transparent" }
                        GradientStop { position: 0.4; color: "transparent" }
                        GradientStop { position: 1.0; color: Theme.rgba(Theme.secondaryColor, 0.1) }
                    }
                }
            }
        }

        Column {
            Label {
                highlighted: item.highlighted ? true : undefined
                text: Math.round(audioMessage.position / 1000) + "s" + audioMessage.durationTenths
                height: attachmentRow.height
                width: attachmentRow.width - playPauseColumn.width - rewindColumn.width - Theme.paddingSmall
                elide: Text.ElideLeft
                verticalAlignment: Text.AlignVCenter
            }
        }
    }
}

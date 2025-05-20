/*
 * This file has been adapted from File Browser for use in Whisperfish.
 *
 * SPDX-FileCopyrightText: 2020-2021 Mirian Margiani
 * SPDX-License-Identifier: GPL-3.0-or-later OR AGPL-3.0-or-later
 */

import QtQuick 2.2
import Sailfish.Silica 1.0
import QtMultimedia 5.0
import "../components"

// TODO video controls
// TODO attached info page

Page {
    id: page
    objectName: "viewVideoPage"

    // For new message notifications
    property int sessionId

    allowedOrientations: Orientation.All
    property alias title: _titleOverlayItem.title
    property alias subtitle: _titleOverlayItem.subtitle
    property MediaTitleOverlay titleOverlay: _titleOverlayItem
    property alias path: video.source
    property alias autoPlay: video.autoPlay
    property bool enableDarkBackground: true
    property var attachment
    property bool isViewOnce

    property bool _isPlaying: video.playbackState === MediaPlayer.PlayingState
    property string _errorString

    Loader {
        sourceComponent: enableDarkBackground ? backgroundComponent : null
        anchors.fill: parent
        Component {
            id: backgroundComponent
            Rectangle {
                visible: enableDarkBackground
                color: Theme.overlayBackgroundColor
                opacity: Theme.opacityHigh
            }
        }
    }

    SilicaFlickable {
        id: flick
        anchors.fill: parent

        PullDownMenu {
            visible: video.playbackState !== MediaPlayer.PlayingState
            MenuItem {
                enabled: attachment.id > 0 && !isViewOnce
                visible: enabled
                //: Copy the attachment video out of Whisperfish
                //% "Export video"
                text: qsTrId("whisperfish-export-video-menu")
                onClicked: {
                    MessageModel.exportAttachment(attachment.id)
                }
            }
        }

        MediaTitleOverlay {
            id: _titleOverlayItem
            shown: video.playbackState !== MediaPlayer.PlayingState

            IconButton {
                anchors.centerIn: parent
                icon.source: "../../icons/play.png?" + (pressed
                            ? Theme.highlightColor
                            : Theme.primaryColor)
                onClicked: mouseArea.onClicked("")
            }

            Rectangle {
                // TODO find a more elegant solution to make
                // this stay below the overlay but above the video
                z: parent.z - 1000
                anchors.fill: parent
                color: Theme.rgba("bbbbbb", 0.5)
            }
        }

        MouseArea {
            id: mouseArea
            anchors.fill: parent
            onClicked: {
                if (_isPlaying === true) {
                    _titleOverlayItem.show();
                    video.pause();
                } else {
                    _titleOverlayItem.hide();
                    video.play();
                }
            }
        }

        Video {
            id: video
            anchors.fill: parent
            autoPlay: false
            fillMode: VideoOutput.PreserveAspectFit
            muted: false
            onStopped: if (autoPlay) { play() }
            onErrorChanged: {
                if (error === MediaPlayer.NoError) return;
                // we don't want to risk crashes by trying any further
                console.log("playing video failed:", errorString)
                _errorString = errorString
                source = ""
                loader.sourceComponent = failedLoading
            }
        }

        Loader {
            id: loader
            anchors.fill: parent
            Component {
                id: failedLoading
                BusyLabel {
                    //: Full page placeholder shown when a video failed to load
                    //% "Failed to play"
                    text: qsTrId("whisperfish-view-video-page-error") +
                        "\n\n" + _errorString
                    running: false
                }
            }
        }
    }
}

// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import be.rubdos.whisperfish 1.0

MouseArea {
    id: root
    property int index: 0
    property var attach: thumbsAttachments.get(index)
    property var attachments: null
    property var message: null
    property bool highlighted: containsPress
    property bool _hasAttach: attach != null
    property bool _isAnimated: _hasAttach ? /\.(gif)$/i.test(attach.data) : false
    property bool _isVideo: _hasAttach ? /^video\//.test(attach.type) : false
    property bool _isAnimatedPaused: false

    Recipient {
        id: recipient
        app: AppState
        recipientId: message ? message.senderRecipientId : -1
    }

    Connections {
        target: attachments
        onDataChanged: {
            var i = topLeft.row;
            if (i != index) {
                return;
            }
            attach = thumbsAttachments.get(i);
        }
    }

    onClicked: {
        if (!_hasAttach) {
            return
        } else if (_isAnimatedPaused && animationLoader.item) {
            _isAnimatedPaused = false
            animationLoader.item.paused = false
        } else {
            var _debugMode = SettingsBridge.debug_mode
            var _viewPage = _isVideo ? '../../pages/ViewVideoPage.qml' : '../../pages/ViewImagePage.qml'

            pageStack.push(Qt.resolvedUrl(_viewPage), {
                'title': message ? recipient.name : "",
                // TODO don't show the file path once attachments work reliably (#many)
                //      and attachments are saved in a WF-controlled directory (#253)
                'subtitle': attach.original_name != null && attach.original_name.length > 0
                    ? attach.original_name
                    : attach.data,
                // when not in debug mode, it is ok to fade the file path if it is too long
                'titleOverlay.subtitleItem.wrapMode': _debugMode ? Text.Wrap : Text.NoWrap,
                'path': attach.data,
                'isAnimated': _isAnimated,
                'attachment': attach,
                'isViewOnce': false, // TODO: Implement attachment can only be viewed once
            })
        }
    }

    // TODO handle missing files and failed thumbnails
    // TODO fix: there are no thumbnails for video files in Whisperfish, even though
    //      the thumbnailer supports videos
    Thumbnail {
        id: nemoThumbnail
        visible: opacity > 0.0
        opacity: (!_isAnimated && attach.data != null && (attach.visual_hash == null || status == Thumbnail.Ready)) ? 1.0 : 0.0
        Behavior on opacity { NumberAnimation { duration: 250 } }
        width: parent.width; height: parent.height
        source: (!_isAnimated && _hasAttach && attach.data != null) ? attach.data : ''
        sourceSize { width: width; height: height }

        onStatusChanged: {
            if (status === Thumbnail.Error && _hasAttach) {
                console.warn("thumbnail failed for", attach.data)
            }
        }
    }

    Image {
        id: blurhashThumb
        visible: opacity > 0.0
        opacity: (!_isAnimated && (nemoThumbnail.status != Thumbnail.Ready) && attach.visual_hash != null) ? 1.0 : 0.0
        Behavior on opacity { NumberAnimation { duration: 250 } }
        width: parent.width; height: parent.height
        source: attach.visual_hash != null ? "image://blurhash/" + attach.visual_hash : "image://theme/icon-m-image"

        BusyIndicator {
            id: downloadingBusyIndicator
            running: blurhashThumb.visible && attach.is_downloading
            anchors.centerIn: blurhashThumb
            size: BusyIndicatorSize.Medium
        }

        Label {
            id: downloadingLabel
            visible: downloadingBusyIndicator.running
            text: Math.round(attach.downloaded_percentage) + " %"
            anchors.centerIn: blurhashThumb
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor
        }
    }

    IconButton {
        id: retryDownloadButton
        enabled: !attach.is_downloaded && attach.can_retry
        visible: enabled
        anchors.centerIn: parent
        icon.source: 'image://theme/icon-m-cloud-download'
        onClicked: {
            console.log("retry download clicked for attachment", attach.id)
            ClientWorker.fetchAttachment(attach.id)
        }
    }

    Loader {
        id: animationLoader
        anchors.fill: parent
        asynchronous: true
        sourceComponent: _isAnimated ? animatedComponent : null
    }

    HighlightImage {
        highlighted: parent.highlighted ? true : undefined
        anchors.centerIn: parent
        width: Theme.iconSizeLarge; height: width
        source: (_isVideo || _isAnimatedPaused) ? 'image://theme/icon-l-play' : ''
    }

    Rectangle {
        anchors.fill: parent
        visible: highlighted
        color: Theme.highlightBackgroundColor
        opacity: Theme.opacityFaint
    }

    Component {
        id: animatedComponent
        AnimatedImage {
            // TODO Find the most intuitive way to show a gif, restart it,
            // and show it on a separate page. Is it ok if the inline view is cropped?
            property int rounds: 0
            property int maxRounds: 2
            fillMode: Image.PreserveAspectCrop
            source: _hasAttach ? attach.data : ''
            onCurrentFrameChanged: if (currentFrame === 0) rounds++
            onRoundsChanged: {
                if (rounds <= maxRounds) return
                rounds = 0
                paused = true
                _isAnimatedPaused = true
            }
        }
    }
}

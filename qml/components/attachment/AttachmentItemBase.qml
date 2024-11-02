// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../../js/attachment.js" as Attachment

MouseArea {
    id: root
    property int index: 0
    property var attach: detailAttachments.get(index)
    property var attachments: null
    property bool highlighted: containsPress
    property string icon: ''
    property bool enableDefaultClickAction: true
    property bool showThumbnail: _hasAttach && !(attach.is_voice_note || (/^audio\//.test(attach.type) && !Attachment.isPlaylist(attach.data)))
    default property alias contents: attachmentContentItem.data

    // check _effectiveEnableClick in derived types, not enableDefaultClickAction
    property bool _effectiveEnableClick: _hasAttach && enableDefaultClickAction
    property bool _hasAttach: attach != null

    function mimeToIcon(mimeType) {
        if (root.icon !== '') return root.icon
        var icon = Theme.iconForMimeType(mimeType)
        return icon === "image://theme/icon-m-file-other" ? "image://theme/icon-m-attach" : icon
    }

    function lastPartOfPath(path) {
        path = path.replace(/\/+/g, '/');
        if (path === "/") return "";
        var i = path.lastIndexOf("/");
        if (i < -1) return path;
        return path.substring(i+1);
    }

    Connections {
        target: attachments
        onDataChanged: {
            var i = topLeft.row;
            if (i != index) {
                return;
            }
            attach = attachments.get(i);
        }
    }

    Row {
        anchors.fill: parent
        spacing: Theme.paddingMedium
        Item {
            id: thumbItem
            height: parent.height
            width: visible ? height : 0
            clip: true
            visible: showThumbnail
            Rectangle {
                anchors { fill: parent; margins: -parent.width/2 }
                rotation: 45
                gradient: Gradient {
                    GradientStop { position: 0.0; color: "transparent" }
                    GradientStop { position: 0.4; color: "transparent" }
                    GradientStop { position: 1.0; color: Theme.rgba(Theme.secondaryColor, 0.1) }
                }
            }

            Thumbnail {
                id: thumb
                anchors.fill: parent
                source: (icon === '' && _hasAttach) ? valueOrEmptyString(attach.data) : ''
                sourceSize { width: width; height: height }
            }
            HighlightImage {
                anchors.centerIn: parent
                highlighted: root.highlighted ? true : undefined
                width: Theme.iconSizeMedium; height: width
                visible: thumb.status === Thumbnail.Error ||
                         thumb.status === Thumbnail.Null
                source: _hasAttach ? mimeToIcon(attach.type) : ''
            }
        }

        Item {
            id: attachmentContentItem
            width: parent.width - thumbItem.width - parent.spacing
            height: parent.height

            /* children... */
        }
    }

    Rectangle {
        anchors.fill: parent
        visible: highlighted
        color: Theme.highlightBackgroundColor
        opacity: Theme.opacityFaint
    }
}

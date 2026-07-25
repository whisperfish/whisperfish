/*
 * ViewImageGalleryPage — swipeable gallery for a message's image attachments.
 *
 * The horizontal swipe ListView structure is adapted from Quickddit's
 * ImageViewPage; each delegate's zoom logic uses PinchZoomImage, which
 * in turn is adapted from ViewImagePage / Quickddit's ImageViewer.
 *
 * SPDX-FileCopyrightText: 2014 Dickson Leong  (Quickddit ImageViewPage)
 * SPDX-FileCopyrightText: 2015-2020 Sander van Grieken  (Quickddit ImageViewPage)
 * SPDX-FileCopyrightText: 2026 Ruben De Smet  (adaptation for Whisperfish)
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"

Page {
    id: page
    objectName: "viewImageGalleryPage"

    // ── Page API ─────────────────────────────────────────────────────
    property int sessionId
    // Aliased to the overlay so the page title/subtitle show up, mirroring
    // ViewImagePage; assignments (incl. from pageStack.push) flow through.
    property alias title: _titleOverlayItem.title
    property alias subtitle: _titleOverlayItem.subtitle
    property MediaTitleOverlay titleOverlay: _titleOverlayItem
    property var attachments
    property int initialIndex: 0
    property var message
    property bool isViewOnce: false
    property var currentAttach: null

    allowedOrientations: Orientation.All

    // ── Derived state ────────────────────────────────────────────────
    property var imageIndexes: []  // indices into attachments whose type starts with "image/"

    readonly property bool currentZoomed: swipeView.currentItem !== null
            ? swipeView.currentItem._pzi.zoomed
            : false

    function currentAttachment() {
        if (swipeView && swipeView.currentIndex >= 0
                && swipeView.currentIndex < imageIndexes.length && attachments)
            return attachments.get(imageIndexes[swipeView.currentIndex]);
        return null;
    }

    function lastPartOfPath(path) {
        var i = path.lastIndexOf("/");
        return i >= 0 ? path.substring(i + 1) : path;
    }

    Component.onCompleted: {
        if (!attachments) return;
        // Build the image indices in a local array, then assign once.
        // Mutating a `var` array with .push() does NOT emit a change
        // signal in QML, so bindings such as `swipeView.model:
        // imageIndexes.length` would otherwise stay bound to the initial
        // empty array and the gallery would render nothing.
        var idxs = [];
        for (var i = 0; i < attachments.count; i++) {
            var a = attachments.get(i);
            if (/^image\//.test(a.type))
                idxs.push(i);
        }
        imageIndexes = idxs;
        // Find position of initialIndex inside imageIndexes
        var start = 0;
        for (var j = 0; j < imageIndexes.length; j++) {
            if (imageIndexes[j] === initialIndex) {
                start = j;
                break;
            }
        }
        swipeView.currentIndex = start;
        // Seed subtitle for initial image
        var initAttach = currentAttachment();
        if (initAttach)
            subtitle = initAttach.original_name || lastPartOfPath(initAttach.data);
    }

    onStatusChanged: {
        if (page.status === PageStatus.Inactive) {
            var item = swipeView.currentItem;
            if (item && item._pzi && item._pzi.imageReady)
                item._pzi.fitToScreen();
        }
    }

    // ── Dark background ──────────────────────────────────────────────
    Loader {
        anchors.fill: parent
        sourceComponent: backgroundComponent
        Component {
            id: backgroundComponent
            Rectangle {
                color: Theme.overlayBackgroundColor
                opacity: Theme.opacityHigh
            }
        }
    }

    // ── Main container (hosts PullDownMenu + swipe ListView) ─────────
    SilicaFlickable {
        id: pageFlickable
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        // ── Title overlay ───────────────────────────────────────────
        MediaTitleOverlay {
            id: _titleOverlayItem
        }

        // ── Pull-down menu ──────────────────────────────────────────
        PullDownMenu {
            MenuItem {
                enabled: currentAttach !== null && currentAttach.id > 0 && !isViewOnce
                visible: enabled
                text: qsTrId("whisperfish-export-image-menu")
                onClicked: if (currentAttach) MessageModel.exportAttachment(currentAttach.id)
            }
        }

        // ── Swipeable horizontal ListView ───────────────────────────
        ListView {
            id: swipeView
            anchors.fill: parent
            orientation: ListView.Horizontal
            snapMode: ListView.SnapOneItem
            boundsBehavior: Flickable.StopAtBounds
            highlightRangeMode: ListView.StrictlyEnforceRange
            preferredHighlightBegin: 0
            preferredHighlightEnd: width
            model: imageIndexes.length
            interactive: count > 1 && !page.currentZoomed
            clip: true

            delegate: Item {
                id: delegateItem
                width: swipeView.width
                height: swipeView.height

                property bool isCurrentImage: swipeView.currentIndex === index
                property alias _pzi: _pinchZoomImage

                PinchZoomImage {
                    id: _pinchZoomImage
                    anchors.fill: parent
                    source: attachments.get(imageIndexes[index]).data
                    isAnimated: attachments.get(imageIndexes[index]).type === "image/gif"
                    paused: page.status !== PageStatus.Active
                            || !Qt.application.active
                            || !isCurrentImage

                    onClicked: {
                        if (_titleOverlayItem.visible)
                            _titleOverlayItem.hide();
                        else
                            _titleOverlayItem.show();
                    }
                }
            }

            onCurrentIndexChanged: {
                var attach = currentAttachment();
                if (attach) {
                    subtitle = attach.original_name || lastPartOfPath(attach.data);
                }
                currentAttach = attach;
            }
        }
    }

    // ── Position indicator (e.g. "3 / 7") ───────────────────────────
    Label {
        anchors {
            bottom: parent.bottom
            horizontalCenter: parent.horizontalCenter
            margins: Theme.paddingMedium
        }
        text: imageIndexes.length > 0 ? (swipeView.currentIndex + 1) + " / " + imageIndexes.length : ""
        visible: text !== "" && imageIndexes.length > 1
        color: Theme.highlightColor
        opacity: Theme.opacityHigh
        font.pixelSize: Theme.fontSizeSmall
    }
}

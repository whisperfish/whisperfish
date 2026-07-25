/*
 * PinchZoomImage — a self-contained, reusable zoomable image Item.
 *
 * The zoom / fit / pinch / animated-image / loading / error logic is
 * adapted from Whisperfish's ViewImagePage (itself adapted from File
 * Browser) and from Quickddit's reusable ImageViewer component.
 *
 * SPDX-FileCopyrightText: 2016 Sander van Grieken  (Quickddit ImageViewer)
 * SPDX-FileCopyrightText: 2020-2021 Mirian Margiani  (ViewImagePage, adapted from File Browser;
 *   pinch/zoom/fit logic and animated-image handling carried over).
 * SPDX-FileCopyrightText: 2026 Ruben De Smet  (packaging as reusable component for Whisperfish)
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: root

    // ── Public API ────────────────────────────────────────────────────
    property url source: ""
    property bool isAnimated: false
    property bool paused: false

    readonly property int status: isAnimated ? (animationLoader.item ? animationLoader.item.status : Image.Null) : image.status
    readonly property bool imageReady: status === Image.Ready
    readonly property bool zoomed: imageReady && (image.scale > (fitScale * 1.01))

    // backing property for the readonly alias
    property real _fitScale: 0
    readonly property real fitScale: _fitScale

    signal clicked()

    function fitToScreen() {
        image.fitToScreen()
    }

    function zoomOut() {
        pinchArea.zoomToScale(root.fitScale, false)
    }

    // ── Internal state ────────────────────────────────────────────────

    // ── Flickable container ───────────────────────────────────────────
    SilicaFlickable {
        id: flick
        anchors.fill: parent
        contentWidth: Math.max(image.width * image.scale, flick.width)
        contentHeight: Math.max(image.height * image.scale, flick.height)
        onHeightChanged: if (root.imageReady) image.fitToScreen()

        // ── Image view item ─────────────────────────────────────────
        Item {
            id: imageView
            width: Math.max(image.width * image.scale, flick.width)
            height: Math.max(image.height * image.scale, flick.height)

            // Animated-image loader (AnimatedImage cannot be asynchronous)
            Loader {
                id: animationLoader
                anchors.fill: image
                asynchronous: true
                sourceComponent: isAnimated ? animationComponent : null

                Component {
                    id: animationComponent
                    AnimatedImage {
                        scale: image.scale
                        fillMode: image.fillMode
                        source: root.source
                        paused: root.paused
                    }
                }
            }

            // Main static image
            Image {
                id: image
                property real prevScale: 0

                function fitToScreen() {
                    scale = Math.min(flick.width / width, flick.height / height)
                    pinchArea.minScale = scale
                    pinchArea.maxScale = 4 * Math.max(flick.width / width, flick.height / height)
                    root._fitScale = scale
                    prevScale = scale
                }

                visible: !isAnimated
                anchors.centerIn: parent
                fillMode: Image.PreserveAspectFit
                cache: false
                autoTransform: true
                asynchronous: true
                smooth: !flick.moving
                opacity: status === Image.Ready ? 1.0 : 0.0
                source: root.source

                Behavior on opacity { FadeAnimator { duration: 250 } }

                onStatusChanged: {
                    if (status === Image.Ready) {
                        fitToScreen()
                        statusLoader.sourceComponent = undefined
                    } else if (status === Image.Loading) {
                        statusLoader.sourceComponent = loadingIndicator
                    } else if (status === Image.Error) {
                        statusLoader.sourceComponent = failedLoading
                    }
                }

                onScaleChanged: {
                    if ((width * scale) > flick.width) {
                        var xoff = (flick.width / 2 + flick.contentX) * scale / prevScale
                        flick.contentX = xoff - flick.width / 2
                    }
                    if ((height * scale) > flick.height) {
                        var yoff = (flick.height / 2 + flick.contentY) * scale / prevScale
                        flick.contentY = yoff - flick.height / 2
                    }
                    prevScale = scale
                    flick.returnToBounds()
                }

                transform: [
                    Rotation {
                        id: imageRotationObj
                        origin { x: image.width / 2; y: image.height / 2 }
                        angle: 0
                    }
                ]
            }
        }

        // ── Pinch area ──────────────────────────────────────────────
        PinchArea {
            id: pinchArea
            property real minScale: 1.0
            property real maxScale: 3.0

            MouseArea {
                property bool pinchRequested: false

                anchors.fill: parent
                Timer { id: singleClickTimer; interval: 200; onTriggered: parent.singleClick() }
                onClicked: singleClickTimer.start()

                onDoubleClicked: {
                    pinchRequested = true
                    if (!root.imageReady) return

                    var newScale = pinchArea.minScale
                    if (Math.round(image.scale) === Math.round(pinchArea.minScale)) {
                        var scaledWidth = Math.round(image.width * image.scale)
                        var scaledHeight = Math.round(image.height * image.scale)
                        var buffer = Theme.horizontalPageMargin

                        if (scaledWidth >= (flick.width - buffer) && scaledWidth <= (flick.width + buffer) &&
                                scaledHeight >= (flick.height - buffer) && scaledHeight <= (flick.height + buffer)) {
                            newScale = pinchArea.maxScale
                        } else if (scaledWidth === flick.width) {
                            newScale = (flick.height - 5) / image.height
                        } else if (scaledHeight === flick.height) {
                            newScale = (flick.width - 5) / image.width
                        }
                    }

                    pinchArea.zoomToScale(newScale, true)
                }

                function singleClick() {
                    if (pinchRequested) {
                        pinchRequested = false
                        return
                    }
                    root.clicked()
                }
            }

            anchors.fill: parent
            enabled: root.imageReady
            pinch.target: image
            pinch.minimumScale: 0.5 * minScale
            pinch.maximumScale: 1.5 * maxScale

            onPinchFinished: {
                flick.returnToBounds()
                if (image.scale < pinchArea.minScale) {
                    zoomToScale(pinchArea.minScale, false)
                } else if (image.scale > pinchArea.maxScale) {
                    zoomToScale(pinchArea.maxScale, false)
                }
            }

            function zoomToScale(newScale, quick) {
                if (quick === true)
                    bounceBackAnimation.quick = true
                else
                    bounceBackAnimation.quick = false
                bounceBackAnimation.to = newScale
                bounceBackAnimation.start()
            }

            NumberAnimation {
                id: bounceBackAnimation
                target: image
                property bool quick: false
                duration: quick ? 150 : 250
                property: "scale"
                from: image.scale
            }
        }
    }

    // ── Status loader ─────────────────────────────────────────────────
    Loader {
        id: statusLoader
        anchors.fill: parent
        sourceComponent: undefined
    }

    Component {
        id: loadingIndicator
        BusyLabel {
            //: Full page placeholder shown while a large image is being loaded
            //% "Loading image"
            text: qsTrId("whisperfish-view-image-page-loading")
            running: true
        }
    }

    Component {
        id: failedLoading
        BusyLabel {
            //: Full page placeholder shown when an image failed to load
            //% "Failed to load"
            text: qsTrId("whisperfish-view-image-page-error")
            running: false
        }
    }
}
